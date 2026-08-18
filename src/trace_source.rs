/*
 * Copyright (c) Kia Shakiba
 *
 * This source code is licensed under the GNU AGPLv3 license found in the
 * LICENSE file in the root directory of this source tree.
 */

//! Where the benchmark's access stream comes from: a local file, stdin, or a
//! TCP socket.
//!
//! ## Why streaming a trace is cheap
//!
//! A trace record is **25 bytes** (`Access::chunk_size()`), and the object's
//! value bytes are *not* in the trace at all — `Access::from_chunk`
//! synthesizes them locally from the record's `value_size` field
//! (`[0u8].repeat(value_size)`). So the network carries only the record
//! stream, never the ~16 KB objects.
//!
//! At this benchmark's measured throughput (~120K accesses/sec against a
//! hybrid cache) that is **~3 MB/s, about 23 Mbit/s**. Even a 100 Mbit link
//! has 4x the headroom needed, and the run stays bound by the cache rather
//! than the wire: cluster12's 2.65B records take ~6 hours to replay
//! regardless of how fast they arrive.
//!
//! That is the whole reason this is worth doing — a 66 GB trace that will not
//! fit on local disk still only needs a few MB/s to replay from wherever it
//! does fit.
//!
//! ## Backpressure is already handled
//!
//! `main` feeds clients through a `bounded(clients)` channel, so the reader
//! blocks as soon as the clients fall behind. On a socket that block
//! propagates as TCP flow control and the sender simply stalls — there is no
//! unbounded buffer anywhere on the path, and a fast link cannot outrun the
//! cache into an OOM.
//!
//! ## What a stream cannot do
//!
//! A file reader can `seek`; a socket cannot. Two things depend on that and
//! are therefore unavailable when streaming:
//!
//! - **The record count.** `BinaryReader::size()` stats the file. A stream
//!   has no length, so `--trace-records` supplies it. This is not cosmetic:
//!   the count pre-sizes `Stats`' latency buffers, and undersizing them has
//!   already caused a real allocation-failure abort on a `-c 4` run (see
//!   `Stats::with_capacity`). Without the hint the run still works, but
//!   progress has no ETA and the buffers grow by doubling.
//! - **`--native-time`**, which reads the first and last records to compute
//!   the trace timespan. Rejected up front rather than half-working.

use std::io::{self, Read, BufReader};
use std::net::{TcpStream, TcpListener};
use std::path::PathBuf;
use std::thread;

use crossbeam_channel::{bounded, Receiver};

use kwik::file::{
    FileReader,
    binary::{BinaryReader, SizedChunk, ReadChunk},
};

use crate::access::Access;

/// Bytes buffered ahead of the decoder. Large enough that a socket read
/// amortizes over ~2600 records rather than costing a syscall each.
const STREAM_BUFFER_BYTES: usize = 256 * 1024;

/// Records held in flight between the network reader thread and the replay
/// loop. At the measured consumption rate (~58K records/sec on this box) this
/// is roughly 4 seconds of buffer, which is what absorbs a network stall
/// without the replay loop ever noticing.
///
/// Sized in records rather than bytes because that is the unit that matters:
/// what must not happen is the feed loop running dry. 256K records is ~6 MB
/// of `Access` structs, negligible next to a multi-GB cache.
const PREFETCH_RECORDS: usize = 256 * 1024;

/// CRC-32 (IEEE, the zlib/gzip polynomial) over every trace byte received.
///
/// TCP's checksum is 16 bits. Over a 66 GB trace that is weak enough that an
/// undetected corruption is a real possibility rather than a theoretical one,
/// and a silently corrupted trace produces a plausible-looking miss ratio
/// with nothing to indicate it is wrong. This is an end-to-end check the
/// sender can reproduce with stock tooling:
///
/// ```text
/// python3 -c "import zlib,sys; c=0
/// f=open('/traces/cluster12.bin','rb')
/// while (b:=f.read(1<<20)): c=zlib.crc32(b,c)
/// print(f'{c:08x}')"
/// ```
///
/// Costs one table lookup per byte -- at 25 bytes per record and this
/// benchmark's ~120K records/sec that is ~3 MB/s of hashing, nothing next to
/// the cache work it runs alongside.
struct Crc32 {
    table: [u32; 256],
    value: u32,
}

impl Crc32 {
    fn new() -> Self {
        let mut table = [0u32; 256];

        for (i, entry) in table.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *entry = c;
        }

        Crc32 { table, value: 0xFFFF_FFFF }
    }

    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            let idx = ((self.value ^ b as u32) & 0xFF) as usize;
            self.value = self.table[idx] ^ (self.value >> 8);
        }
    }

    fn finish(&self) -> u32 {
        self.value ^ 0xFFFF_FFFF
    }
}

/// Where to read the access stream from.
pub enum TraceSource {
    /// A local trace file — the original behavior, still the default.
    Path(PathBuf),
    /// Standard input. Pairs with anything that can write the trace to a
    /// pipe, which is the recommended way to do this:
    ///
    /// ```text
    /// ssh host 'cat /traces/cluster12.bin' | paper-benchmark --trace-stdin ...
    /// ```
    ///
    /// ssh supplies transport, auth, and (via the pipe) backpressure, so
    /// nothing here has to. Add `-C`, or pipe through `zstd`, if the link is
    /// slower than ~25 Mbit/s.
    Stdin,
    /// Connect out to `host:port` and read the raw record stream, for a
    /// sender like `nc -l -p 9999 < trace.bin` or `socat`.
    Tcp(String),
    /// Bind `addr` and accept one inbound connection, so the machine holding
    /// the trace pushes to this one:
    ///
    /// ```text
    /// # here
    /// paper-benchmark --trace-listen 0.0.0.0:9999 --trace-records N ...
    /// # on the machine with the trace
    /// cat /traces/cluster12.bin | nc benchmark-host 9999
    /// ```
    ///
    /// Usually the easier direction: only this side needs an open port, and
    /// the trace never has to be reachable from here.
    Listen(String),
}

impl TraceSource {
    /// Opens the source and returns an iterator over its accesses.
    ///
    /// `records` is the caller's record-count hint (`--trace-records`), used
    /// only for the streaming variants; a file knows its own length.
    /// Returns the iterator plus the record count if it is known.
    pub fn open(
        &self,
        records: Option<u64>,
        limit: Option<u64>,
    ) -> io::Result<(Box<dyn Iterator<Item = Access>>, Option<u64>)> {
        let (iter, known): (Box<dyn Iterator<Item = Access>>, Option<u64>) = match self {
            TraceSource::Path(path) => {
                let reader = BinaryReader::<Access>::from_path(path)?;
                let count = reader.size() / Access::chunk_size() as u64;
                (Box::new(reader.into_iter()), Some(count))
            },

            // Both streaming variants below hand their decoder to
            // `prefetch`, which moves the blocking socket/pipe read onto its
            // own thread. Without it the read sits on the same thread as the
            // replay loop, so every network stall -- a retransmit, a
            // scheduling delay on the sender, a momentary bandwidth dip --
            // propagates straight into the client feed and pauses the
            // benchmark. That is not a throughput problem (the wire needs
            // only ~1.4 MB/s) but a jitter one, and jitter demonstrably
            // moves the measured miss ratio on this crate: a co-located
            // producer competing for CPU shifted uniform_baseline from
            // ~0.558 to ~0.60-0.64 purely by changing how evenly accesses
            // arrived. A file source needs none of this -- page-cache reads
            // do not stall in the same way -- so it is left alone.
            TraceSource::Stdin => (
                // Unlocked `io::stdin()` rather than `.lock()`: `StdinLock`
                // is not `Send`, so it cannot move to the prefetch thread.
                // The per-read mutex it avoids was measured to make no
                // difference anyway (locking stdin left uniform_baseline at
                // 0.601-0.602, unchanged), and with the read now on its own
                // thread that cost is off the replay path entirely.
                prefetch(ChunkIter::new(BufReader::with_capacity(
                    STREAM_BUFFER_BYTES,
                    io::stdin(),
                ))),
                records,
            ),

            TraceSource::Tcp(addr) => {
                let stream = TcpStream::connect(addr)?;
                // Nagle would batch our (nonexistent) writes; we only read,
                // but disabling it keeps the sender's small final flush from
                // stalling at the end of the trace.
                let _ = stream.set_nodelay(true);
                (
                    prefetch(ChunkIter::new(BufReader::with_capacity(
                        STREAM_BUFFER_BYTES,
                        stream,
                    ))),
                    records,
                )
            },

            TraceSource::Listen(addr) => {
                let listener = TcpListener::bind(addr)?;
                println!("Waiting for a trace sender on {addr} ...");

                let (stream, peer) = listener.accept()?;
                let _ = stream.set_nodelay(true);
                println!("Trace sender connected from {peer}");

                (
                    prefetch(ChunkIter::new(BufReader::with_capacity(
                        STREAM_BUFFER_BYTES,
                        stream,
                    ))),
                    records,
                )
            },
        };

        match limit {
            Some(n) => Ok((Box::new(iter.take(n as usize)), Some(known.map_or(n, |k| k.min(n))))),
            None => Ok((iter, known)),
        }
    }

    /// Whether this source supports the seeks `--native-time` needs.
    pub fn is_seekable(&self) -> bool {
        matches!(self, TraceSource::Path(_))
    }

    pub fn describe(&self) -> String {
        match self {
            TraceSource::Path(p) => format!("file {}", p.display()),
            TraceSource::Stdin => "stdin".to_string(),
            TraceSource::Tcp(addr) => format!("tcp {addr}"),
            TraceSource::Listen(addr) => format!("tcp listen {addr}"),
        }
    }
}

/// Decodes a byte stream into fixed-size `Access` records.
///
/// `read_exact` is what makes this correct over TCP: a socket read may return
/// fewer bytes than asked for at any time, so decoding whatever a single
/// `read` happened to deliver would tear records apart and desynchronize the
/// stream permanently. `read_exact` loops until the full 25 bytes arrive.
///
/// Ends cleanly at EOF. A *partial* record at EOF (`UnexpectedEof` after some
/// bytes) means the sender died mid-record or the trace is truncated: that is
/// reported and the iteration stops, rather than being silently treated as a
/// normal end of stream.
struct ChunkIter<R: Read> {
    reader: R,
    buf: [u8; 32],
    done: bool,
    decoded: u64,
    crc: Crc32,
}

impl<R: Read> ChunkIter<R> {
    fn new(reader: R) -> Self {
        assert!(Access::chunk_size() <= 32, "record larger than the decode buffer");

        ChunkIter {
            reader,
            buf: [0u8; 32],
            done: false,
            decoded: 0,
            crc: Crc32::new(),
        }
    }
}

impl<R: Read> Iterator for ChunkIter<R> {
    type Item = Access;

    fn next(&mut self) -> Option<Access> {
        if self.done {
            return None;
        }

        let size = Access::chunk_size();

        match self.reader.read_exact(&mut self.buf[..size]) {
            Ok(()) => {},

            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                // Clean EOF on a record boundary is the normal end of a
                // trace and needs no comment. `read_exact` cannot tell us
                // how many bytes it consumed, so a torn tail is
                // indistinguishable here -- the record count printed at the
                // end of the run is what surfaces a short stream.
                self.done = true;
                self.report_integrity();
                return None;
            },

            Err(err) => {
                eprintln!(
                    "\ntrace stream failed after {} records: {err}",
                    self.decoded,
                );
                self.done = true;
                self.report_integrity();
                return None;
            },
        }

        self.crc.update(&self.buf[..size]);

        match Access::from_chunk(&self.buf[..size]) {
            Ok(access) => {
                self.decoded += 1;
                Some(access)
            },

            // The command byte must be 0 or 1, so this fires on any
            // corruption or byte-misalignment that lands there -- a cheap,
            // always-on structural check that catches a desynchronized
            // stream immediately instead of letting it replay garbage keys.
            Err(err) => {
                eprintln!(
                    "\nCORRUPT trace record at index {}: {err} -- stopping. \
                     Raw bytes: {:02x?}",
                    self.decoded, &self.buf[..size],
                );
                self.done = true;
                self.report_integrity();
                None
            },
        }
    }
}

impl<R: Read> ChunkIter<R> {
    /// Printed once when the stream ends, for comparison against the same
    /// CRC computed over the source file -- see `Crc32`'s doc comment for
    /// the one-liner that produces it.
    fn report_integrity(&self) {
        println!(
            "\nTrace stream integrity: {} records, crc32={:08x}",
            self.decoded,
            self.crc.finish(),
        );
    }
}

/// Moves a blocking record decoder onto its own thread, handing records to
/// the caller through a bounded channel.
///
/// The channel bound is what makes this safe in both directions: the reader
/// thread blocks once `PREFETCH_RECORDS` are queued, so a fast link cannot
/// run ahead into unbounded memory (that backpressure reaches the sender as
/// TCP flow control), while the replay loop keeps draining a full queue
/// through any stall shorter than the buffer depth.
///
/// The thread is detached. When the replay loop stops early -- `--trace-limit`
/// -- the receiver drops, the channel disconnects, and the reader's next send
/// fails and ends the thread.
fn prefetch<I>(iter: I) -> Box<dyn Iterator<Item = Access>>
where
    I: Iterator<Item = Access> + Send + 'static,
{
    let (tx, rx): (_, Receiver<Access>) = bounded(PREFETCH_RECORDS);

    thread::spawn(move || {
        for access in iter {
            if tx.send(access).is_err() {
                // Replay loop went away (e.g. --trace-limit reached).
                break;
            }
        }
    });

    Box::new(rx.into_iter())
}
