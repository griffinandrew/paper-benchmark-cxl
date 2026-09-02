/*
 * trace_fill -- resolve zero-size records in a binary access trace.
 *
 * WHY THIS EXISTS
 *
 * The Twitter cluster traces record a value size per request. For a GET that
 * MISSED in production there was no value to return, so the field is 0. The
 * benchmark's read-through client fills the cache on a miss using that same
 * record's size, so a 0 there admits a zero-byte object: the request is
 * replayed, but nothing of the right shape is ever stored, and the object's
 * real footprint never enters the cache.
 *
 * Dropping those records instead is worse -- they are real lookups, they drive
 * the miss ratio, and removing them changes the reference stream.
 *
 * So: keep every record, and give each zero-size record the size and TTL the
 * object is known to have from its nearest non-zero appearance in the trace.
 *
 * WHY OFFLINE, AS A TRACE REWRITE
 *
 * The alternative is a per-key side table inside the benchmark process. That
 * table would be hundreds of millions of entries on the large clusters, and it
 * would allocate inside the very process whose jemalloc `stats.allocated` is
 * the measurement. Rewriting the trace once keeps the replay harness and its
 * memory accounting untouched, and makes the transformation inspectable and
 * reproducible rather than a runtime behaviour.
 *
 * RESOLUTION RULE
 *
 * For a record with size 0, adopt (size, ttl) from:
 *   1. the most recent PRECEDING record for that key with a non-zero size; else
 *   2. the nearest FOLLOWING such record (equivalently, the first one in the
 *      trace, since by (1) there is none before); else
 *   3. the configured fallback.
 *
 * Implemented in two passes over one table. Pass 1 records the FIRST non-zero
 * (size, ttl) per key. Pass 2 walks the trace overwriting each key's entry as
 * it passes non-zero records, so at any point a lookup returns the most recent
 * preceding one if there is one, and otherwise still holds pass 1's first-ever
 * value -- which is exactly the nearest following one. One table, both rules.
 *
 * FILE FORMAT
 *
 * A flat array of 25-byte little-endian records, no header:
 *   timestamp u64 | command u8 (0=GET, 1=SET) | key u64 | value_size u32 | ttl u32
 * `ttl == 0` means "no TTL". Matches `Access` in paper-benchmark's access.rs.
 */

use std::env;
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::process;

const CHUNK: usize = 25;
const CMD_GET: u8 = 0;
const CMD_SET: u8 = 1;

/// Records per IO block. 40,000 * 25 = 1,000,000 bytes.
const BLOCK_RECORDS: usize = 40_000;

// ---------------------------------------------------------------- record ---

#[derive(Clone, Copy)]
struct Record {
	timestamp: u64,
	command: u8,
	key: u64,
	size: u32,
	ttl: u32,
}

impl Record {
	#[inline]
	fn decode(b: &[u8]) -> Record {
		Record {
			timestamp: u64::from_le_bytes(b[0..8].try_into().unwrap()),
			command: b[8],
			key: u64::from_le_bytes(b[9..17].try_into().unwrap()),
			size: u32::from_le_bytes(b[17..21].try_into().unwrap()),
			ttl: u32::from_le_bytes(b[21..25].try_into().unwrap()),
		}
	}

	#[inline]
	fn encode(&self, b: &mut [u8]) {
		b[0..8].copy_from_slice(&self.timestamp.to_le_bytes());
		b[8] = self.command;
		b[9..17].copy_from_slice(&self.key.to_le_bytes());
		b[17..21].copy_from_slice(&self.size.to_le_bytes());
		b[21..25].copy_from_slice(&self.ttl.to_le_bytes());
	}
}

// ----------------------------------------------------------- size table ---

/// Open-addressed `key -> (size, ttl)`, 16 bytes a slot.
///
/// A `std::HashMap<u64, (u32, u32)>` costs about twice this, which matters at
/// 140M distinct keys (cluster19). `size == 0` marks an empty slot, which is
/// sound because only non-zero sizes are ever inserted -- that is the whole
/// point of the table.
#[derive(Clone, Copy)]
struct Slot {
	key: u64,
	size: u32,
	ttl: u32,
}

struct SizeTable {
	slots: Vec<Slot>,
	mask: usize,
	len: usize,
}

#[inline]
fn mix(mut x: u64) -> u64 {
	// splitmix64 finalizer. Trace keys are already hashes, but they are used
	// directly as cache keys elsewhere and their low bits are not guaranteed
	// well spread for linear probing.
	x ^= x >> 30;
	x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
	x ^= x >> 27;
	x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
	x ^ (x >> 31)
}

impl SizeTable {
	fn with_capacity(cap: usize) -> SizeTable {
		let cap = cap.next_power_of_two().max(1024);

		SizeTable {
			slots: vec![Slot { key: 0, size: 0, ttl: 0 }; cap],
			mask: cap - 1,
			len: 0,
		}
	}

	#[inline]
	fn probe(&self, key: u64) -> usize {
		let mut i = mix(key) as usize & self.mask;

		loop {
			let s = &self.slots[i];

			if s.size == 0 || s.key == key {
				return i;
			}

			i = (i + 1) & self.mask;
		}
	}

	#[inline]
	fn get(&self, key: u64) -> Option<(u32, u32)> {
		let s = &self.slots[self.probe(key)];

		match s.size {
			0 => None,
			size => Some((size, s.ttl)),
		}
	}

	/// Inserts, overwriting any existing entry for `key`.
	#[inline]
	fn set(&mut self, key: u64, size: u32, ttl: u32) {
		debug_assert!(size != 0, "zero size would read back as an empty slot");

		if (self.len + 1) * 10 >= self.slots.len() * 7 {
			self.grow();
		}

		let i = self.probe(key);

		if self.slots[i].size == 0 {
			self.len += 1;
		}

		self.slots[i] = Slot { key, size, ttl };
	}

	/// Offers a sized record as a donor, resolving size and TTL under
	/// SEPARATE rules.
	///
	/// Size follows `pick`. TTL cannot: `ttl == 0` in this format means "this
	/// record carries none", and every GET record in the Twitter traces has
	/// it. With `--source any` the size donor is therefore often a record with
	/// no TTL, and letting it win would erase a real TTL found earlier on a
	/// SET. So a zero never overwrites a non-zero, in either direction.
	#[inline]
	fn offer(&mut self, key: u64, size: u32, ttl: u32, pick: GlobalPick) {
		debug_assert!(size != 0, "zero size would read back as an empty slot");

		let Some((have_size, have_ttl)) = self.get(key) else {
			self.set(key, size, ttl);
			return;
		};

		let takes_size = match pick {
			GlobalPick::First => false,
			GlobalPick::Last => true,
			GlobalPick::Max => size > have_size,
		};

		let new_size = match takes_size {
			true => size,
			false => have_size,
		};

		let new_ttl = match (ttl, have_ttl) {
			// Donor has no TTL: never let it clear one we already found.
			(0, have) => have,
			// We have none yet, or the size donor won: take the donor's.
			(offered, 0) => offered,
			(offered, have) => match takes_size {
				true => offered,
				false => have,
			},
		};

		self.set(key, new_size, new_ttl);
	}

	fn grow(&mut self) {
		let mut bigger = SizeTable::with_capacity(self.slots.len() * 2);

		for s in &self.slots {
			if s.size != 0 {
				let i = bigger.probe(s.key);
				bigger.slots[i] = *s;
				bigger.len += 1;
			}
		}

		*self = bigger;
	}
}

// -------------------------------------------------------------- options ---

#[derive(Clone, Copy, PartialEq)]
enum Patch {
	/// Only GET records with size 0. These are the production misses whose
	/// size drives the read-through fill.
	Gets,
	/// GET and SET records with size 0.
	All,
}

#[derive(Clone, Copy, PartialEq)]
enum Resolve {
	/// Most recent PRECEDING non-zero size for the key, falling forward to the
	/// nearest following one when the key has none before. A key's size may
	/// therefore change over the trace, tracking the real writes.
	Nearest,
	/// One size and TTL per key for the whole trace, taken from its first
	/// sized record. This is what Sari's traces contain: across 10.7M repeated
	/// keys in cluster13 and 11.1M in cluster19, not one key's size or TTL
	/// ever changes.
	Global,
}

/// Which of a key's sized records supplies the one global (size, ttl).
///
/// Only meaningful with `--resolve global`, and only matters for keys written
/// more than once with differing values. On cluster13 that is 60,031 keys out
/// of 74.8M, so the choice is noise; on cluster19 1,942,184 repeat-SETs carry
/// a different TTL than the previous SET for the same key, so `global`
/// genuinely discards TTL variation there whichever end is picked.
#[derive(Clone, Copy, PartialEq)]
enum GlobalPick {
	First,
	Last,
	/// The largest size the key is ever seen at, with the TTL from that same
	/// record. Worst-case footprint, so a fill can never under-provision, and
	/// it does not depend on where in the trace the key is looked at.
	Max,
}

#[derive(Clone, Copy, PartialEq)]
enum Source {
	/// Any record with a non-zero size may donate.
	Any,
	/// Only SET records donate. Sari's per-key table comes from the writes --
	/// his cluster19 TTL histogram totals 503,599,311, exactly kia's SET
	/// count for that cluster.
	Sets,
}

#[derive(Clone, Copy, PartialEq)]
enum TtlSource {
	/// Take the TTL from the same record the size came from. This is the
	/// default: a size and the TTL it was stored under belong together.
	Donor,
	/// Keep whatever TTL the zero-size record already carried.
	Record,
}

#[derive(Clone, Copy)]
enum Fallback {
	/// Leave the record at size 0 and count it. Non-destructive default.
	Keep,
	/// Use the median non-zero value size over the processed span.
	Median,
	/// Use a fixed size.
	Fixed(u32),
	/// Omit the record from the output. Changes the reference stream, so it
	/// is never the default.
	Drop,
}

struct Opts {
	input: String,
	output: String,
	patch: Patch,
	ttl: TtlSource,
	fallback: Fallback,
	limit: Option<u64>,
	resolve: Resolve,
	source: Source,
	global_pick: GlobalPick,
	/// Apply the key's size to every record, not only the zero-size ones.
	override_size: bool,
	/// Apply the key's TTL to every record. Independent of `override_size`,
	/// because the common case wants exactly that split: a GET that already
	/// reported a size should keep its own real size, but it still carries
	/// `ttl == 0` and needs one so a miss can fill without a side lookup.
	override_ttl: bool,
	/// Bytes added to every emitted size, to approximate Sari's key+value
	/// convention. Applied last, after resolution and fallback, and never to
	/// a record left at size 0.
	size_add: u32,
	/// Emit only GET records, dropping the SETs. This is the shape Sari's
	/// 20-byte trace has -- every record is a reference and there is no
	/// command field, so the cache is driven read-through and every write
	/// comes from a miss. SET records are still READ, because they are where
	/// the true sizes come from; they are just not replayed.
	gets_only: bool,
}

// ----------------------------------------------------------------- crc32 ---

/// CRC-32 (IEEE), matching the check `trace_source.rs` runs over a streamed
/// trace, so a rewritten trace can be verified end to end the same way.
struct Crc32 {
	table: [u32; 256],
	value: u32,
}

impl Crc32 {
	fn new() -> Crc32 {
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

	#[inline]
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

// ------------------------------------------------------------- histogram ---

/// Exact counts for sizes below 1 MiB, an explicit list above it. Only needed
/// for `--fallback median`.
struct Sizes {
	small: Vec<u64>,
	large: Vec<u32>,
	count: u64,
}

const SMALL_MAX: usize = 1 << 20;

impl Sizes {
	fn new() -> Sizes {
		Sizes { small: vec![0; SMALL_MAX], large: Vec::new(), count: 0 }
	}

	#[inline]
	fn add(&mut self, size: u32) {
		self.count += 1;

		match (size as usize) < SMALL_MAX {
			true => self.small[size as usize] += 1,
			false => self.large.push(size),
		}
	}

	fn median(&mut self) -> Option<u32> {
		if self.count == 0 {
			return None;
		}

		let target = self.count / 2;
		let mut seen = 0u64;

		for (size, &n) in self.small.iter().enumerate() {
			seen += n;

			if seen > target {
				return Some(size as u32);
			}
		}

		self.large.sort_unstable();
		let into_large = (target - seen) as usize;

		self.large.get(into_large).copied()
	}
}

// ------------------------------------------------------------------ main ---

fn main() {
	let opts = match parse_args() {
		Ok(opts) => opts,
		Err(msg) => {
			eprintln!("trace_fill: {msg}\n");
			eprintln!("{USAGE}");
			process::exit(2);
		},
	};

	if let Err(e) = run(&opts) {
		eprintln!("trace_fill: {e}");
		process::exit(1);
	}
}

const USAGE: &str = "\
usage: trace_fill <input.bin> <output.bin> [options]

  --patch gets|all       which zero-size records to resolve (default: gets)
  --ttl donor|record     where the filled TTL comes from (default: donor)
  --fallback keep|median|drop|<bytes>
                         what to do for a key with no non-zero size anywhere
                         in the processed span (default: keep)
  --limit <n>            process only the first n records; the resolution
                         table is built over the same span, so a prefix run
                         never borrows a size from beyond its own end
  --gets-only            emit only GET records. SETs are still read for their
                         sizes, but not replayed, so every write comes from a
                         miss. This is the shape of Sari's 20-byte trace.
  --resolve nearest|global
                         nearest: most recent preceding size for the key,
                         falling forward when there is none before, so a key's
                         size tracks its real writes (default).
                         global:  one size and TTL per key for the whole
                         trace, from its first sighting. Sari's rule.
  --source any|sets      which records may donate a size (default: any).
                         Sari's table is built from the SETs.
  --global-pick first|last|max
                         with --resolve global, which of a key's sized records
                         supplies its one (size, ttl). Default: first.
                         max = the largest size the key is ever seen at, and
                         the TTL from that same record.

  --override             both of the next two.
  --override-size        apply the key's size to EVERY emitted record.
  --override-ttl         apply the key's TTL to EVERY emitted record. Usually
                         wanted on its own: a GET that reported a real size
                         should keep it, but still needs a TTL.

  --maxsize              the replay this benchmark wants: --gets-only
                         --resolve global --global-pick max --source any
                         --override-ttl --fallback keep. SETs stripped, every
                         GET carries a TTL, zero-size GETs take the key's
                         largest observed size, sized GETs keep their own.
  --size-add <n>         add n bytes to every emitted size. See below.

NOTE ON SIZE: Sari's ValueSize is the key's length PLUS the value's, while
this 25-byte format carries the value alone. Measured against his cluster13,
the gap is exactly +44 at every percentile from p0 to p100 (mean 44.246); on
cluster19 it varies per record (min +24, p50 +42, mean +42.34), because the
key length is not constant there. So `--size-add 44` reproduces cluster13
closely and cluster19 only on average -- the exact key lengths are in the
original Twitter CSVs, not in this binary format, so an exact reproduction has
to be regenerated from those.

Which convention is right depends on the question. The cache keys on a u64
hash and never stores the key bytes, so the value alone is what it holds; but
per-object metadata expressed as a fraction of object size is ~40% smaller on
cluster19 if the key's ~42 bytes count toward the object.

NOTE ON TTL: GET records in these traces carry ttl == 0 without exception --
100% of them, in both cluster13 and cluster19. A GET has no TTL of its own, so
a GET's TTL can only come from a SET. That is why --ttl donor is the default;
--ttl record would leave every filled GET with no TTL at all.

It is also why --override matters. Patching only the zero-size records leaves
every GET that already had a size with ttl == 0 -- on cluster13 that is
151,075,072 of 306,714,865 emitted records with no TTL, and it breaks the
per-key size constancy that Sari's traces have. --sari includes --override.

  --sari                 shorthand for the closest reproduction of Sari's
                         traces: --gets-only --resolve global --source sets
                         --fallback drop
";

/// Pulls the value that follows `--flag`, advancing the cursor past it.
fn value_of(argv: &[String], i: &mut usize, name: &str) -> Result<String, String> {
	*i += 1;
	argv.get(*i).cloned().ok_or_else(|| format!("{name} needs a value"))
}

fn parse_args() -> Result<Opts, String> {
	let argv: Vec<String> = env::args().skip(1).collect();
	let mut positional = Vec::new();

	let mut patch = Patch::Gets;
	let mut ttl = TtlSource::Donor;
	let mut fallback = Fallback::Keep;
	let mut limit = None;
	let mut gets_only = false;
	let mut resolve = Resolve::Nearest;
	let mut source = Source::Any;
	let mut global_pick = GlobalPick::First;
	let mut override_size = false;
	let mut override_ttl = false;
	let mut size_add = 0u32;

	let mut i = 0;

	while i < argv.len() {
		let arg = argv[i].clone();

		match arg.as_str() {
			"--patch" => {
				patch = match value_of(&argv, &mut i, "--patch")?.as_str() {
					"gets" => Patch::Gets,
					"all" => Patch::All,
					other => return Err(format!("unknown --patch {other}")),
				}
			},

			"--ttl" => {
				ttl = match value_of(&argv, &mut i, "--ttl")?.as_str() {
					"donor" => TtlSource::Donor,
					"record" => TtlSource::Record,
					other => return Err(format!("unknown --ttl {other}")),
				}
			},

			"--fallback" => {
				fallback = match value_of(&argv, &mut i, "--fallback")?.as_str() {
					"keep" => Fallback::Keep,
					"median" => Fallback::Median,
					"drop" => Fallback::Drop,
					n => Fallback::Fixed(
						n.parse().map_err(|_| format!("bad --fallback {n}"))?,
					),
				}
			},

			"--limit" => {
				limit = Some(
					value_of(&argv, &mut i, "--limit")?
						.parse()
						.map_err(|_| "bad --limit".to_string())?,
				)
			},

			"--resolve" => {
				resolve = match value_of(&argv, &mut i, "--resolve")?.as_str() {
					"nearest" => Resolve::Nearest,
					"global" => Resolve::Global,
					other => return Err(format!("unknown --resolve {other}")),
				}
			},

			"--source" => {
				source = match value_of(&argv, &mut i, "--source")?.as_str() {
					"any" => Source::Any,
					"sets" => Source::Sets,
					other => return Err(format!("unknown --source {other}")),
				}
			},

			"--global-pick" => {
				global_pick = match value_of(&argv, &mut i, "--global-pick")?.as_str() {
					"first" => GlobalPick::First,
					"last" => GlobalPick::Last,
					other => return Err(format!("unknown --global-pick {other}")),
				}
			},

			"--gets-only" => gets_only = true,

			"--override" => {
				override_size = true;
				override_ttl = true;
			},

			"--override-size" => override_size = true,
			"--override-ttl" => override_ttl = true,

			"--size-add" => {
				size_add = value_of(&argv, &mut i, "--size-add")?
					.parse()
					.map_err(|_| "bad --size-add".to_string())?
			},

			"--sari" => {
				gets_only = true;
				resolve = Resolve::Global;
				source = Source::Sets;
				fallback = Fallback::Drop;
				override_size = true;
				override_ttl = true;
			},

			// The replay this benchmark actually wants: SETs stripped, every
			// GET carrying a TTL so a miss can fill without a side lookup, and
			// a zero-size GET given the largest size that key is ever seen at.
			// A GET that reported a real size keeps it.
			"--maxsize" => {
				gets_only = true;
				resolve = Resolve::Global;
				global_pick = GlobalPick::Max;
				source = Source::Any;
				override_ttl = true;
				fallback = Fallback::Keep;
			},

			"-h" | "--help" => {
				println!("{USAGE}");
				process::exit(0);
			},

			other if other.starts_with('-') => {
				return Err(format!("unknown option {other}"));
			},

			other => positional.push(other.to_string()),
		}

		i += 1;
	}

	if positional.len() != 2 {
		return Err("expected an input and an output path".into());
	}

	Ok(Opts {
		input: positional[0].clone(),
		output: positional[1].clone(),
		patch,
		ttl,
		fallback,
		limit,
		resolve,
		source,
		global_pick,
		override_size,
		override_ttl,
		size_add,
		gets_only,
	})
}

/// Calls `f` for every record in the file, up to `limit`.
fn for_each_record<F>(path: &str, limit: Option<u64>, mut f: F) -> io::Result<u64>
where
	F: FnMut(Record),
{
	let mut file = File::open(path)?;
	let mut buf = vec![0u8; BLOCK_RECORDS * CHUNK];
	let mut seen = 0u64;

	loop {
		// A trailing partial record means a truncated trace; the leftover
		// bytes are dropped and the count reported by the caller will not
		// match the file size, which is the signal that something is wrong.
		let mut filled = 0;

		while filled < buf.len() {
			match file.read(&mut buf[filled..])? {
				0 => break,
				n => filled += n,
			}
		}

		if filled < CHUNK {
			return Ok(seen);
		}

		for chunk in buf[..filled].chunks_exact(CHUNK) {
			if limit.is_some_and(|l| seen >= l) {
				return Ok(seen);
			}

			f(Record::decode(chunk));
			seen += 1;
		}

		if filled < buf.len() {
			return Ok(seen);
		}
	}
}

struct Report {
	records: u64,
	gets: u64,
	sets: u64,
	zero_gets: u64,
	zero_sets: u64,
	in_scope: u64,
	from_preceding: u64,
	from_following: u64,
	unresolved: u64,
	dropped: u64,
	skipped_sets: u64,
	written: u64,
}

fn run(opts: &Opts) -> io::Result<()> {
	// -- pass 1: first non-zero (size, ttl) per key, over the same span the
	//    output will cover.
	eprintln!("pass 1/2: indexing sizes ...");

	let mut table = SizeTable::with_capacity(1 << 20);
	let mut sizes = Sizes::new();
	let want_median = matches!(opts.fallback, Fallback::Median);

	// Whether this record may donate a size and TTL to zero-size records.
	let donates = |r: &Record| {
		r.size != 0 && (opts.source == Source::Any || r.command == CMD_SET)
	};

	let scanned = for_each_record(&opts.input, opts.limit, |r| {
		if donates(&r) {
			// `nearest` needs the FIRST sighting here -- pass 2 walks the
			// table forward from it, and the first is what a record before any
			// of the key's writes must fall forward to. Only `global` gets a
			// say in which sighting wins.
			let pick = match opts.resolve {
				Resolve::Global => opts.global_pick,
				Resolve::Nearest => GlobalPick::First,
			};

			table.offer(r.key, r.size, r.ttl, pick);

			if want_median {
				sizes.add(r.size);
			}
		}
	})?;

	let median = sizes.median();

	eprintln!(
		"  {} records, {} keys with a known size",
		commas(scanned),
		commas(table.len as u64),
	);

	if want_median {
		match median {
			Some(m) => eprintln!("  median non-zero size: {m} B"),
			None => eprintln!("  median non-zero size: none (no sized records)"),
		}
	}

	// -- pass 2: resolve and write.
	eprintln!("pass 2/2: writing {} ...", opts.output);

	let mut out = BufWriter::with_capacity(
		BLOCK_RECORDS * CHUNK,
		File::create(&opts.output)?,
	);

	let mut crc = Crc32::new();
	let mut enc = [0u8; CHUNK];
	let mut err = None;

	let mut rep = Report {
		records: 0,
		gets: 0,
		sets: 0,
		zero_gets: 0,
		zero_sets: 0,
		in_scope: 0,
		from_preceding: 0,
		from_following: 0,
		unresolved: 0,
		dropped: 0,
		skipped_sets: 0,
		written: 0,
	};

	// Keys whose entry pass 2 has already overwritten, i.e. for which a
	// non-zero record has been PASSED rather than merely indexed. That is what
	// separates rule 1 (most recent preceding) from rule 2 (nearest
	// following) in the report; both read the same table.
	let mut passed = SizeTable::with_capacity(1 << 20);

	for_each_record(&opts.input, opts.limit, |r| {
		if err.is_some() {
			return;
		}

		rep.records += 1;

		match r.command {
			CMD_GET => rep.gets += 1,
			CMD_SET => rep.sets += 1,
			_ => {},
		}

		let mut out_record = r;

		if r.size != 0 {
			// Under `nearest` this becomes the most recent preceding size for
			// every later record with this key. Under `global` the table is
			// frozen at pass 1's first-sighting, so a key keeps one size and
			// one TTL for the whole trace.
			if opts.resolve == Resolve::Nearest && donates(&r) {
				// `Last` so the most recent sighting wins the size, while
				// `offer` still refuses to clear a TTL with a GET's zero.
				table.offer(r.key, r.size, r.ttl, GlobalPick::Last);
				passed.set(r.key, 1, 0);
			}
		} else {
			match r.command {
				CMD_GET => rep.zero_gets += 1,
				CMD_SET => rep.zero_sets += 1,
				_ => {},
			}
		}

		// Which records take the key's (size, ttl).
		//
		// `--override` applies it to EVERY record, not just the zero-size
		// ones. Two things make that necessary to match Sari. A GET that
		// already carries a size never enters the zero-size scope, so without
		// override it keeps its own size and a key's size is not constant
		// across the trace. And every GET in these traces carries `ttl == 0`
		// without exception, so a GET left unpatched has no TTL at all --
		// which on cluster13 left 151,075,072 of 306,714,865 emitted records
		// unexpirable.
		let zero_scope = r.size == 0
			&& match opts.patch {
				Patch::Gets => r.command == CMD_GET,
				Patch::All => true,
			};

		let size_scope = opts.override_size || zero_scope;
		let ttl_scope = opts.override_ttl || zero_scope;

		if size_scope || ttl_scope {
			rep.in_scope += 1;

			match table.get(r.key) {
				Some((size, donor_ttl)) => {
					match passed.get(r.key).is_some() {
						true => rep.from_preceding += 1,
						false => rep.from_following += 1,
					}

					if size_scope {
						out_record.size = size;
					}

					if ttl_scope && opts.ttl == TtlSource::Donor {
						out_record.ttl = donor_ttl;
					}
				},

				None => {
					rep.unresolved += 1;

					// Fallback only ever supplies a size; a key with no sized
					// record anywhere has no TTL to inherit either.
					match opts.fallback {
						Fallback::Keep => {},
						Fallback::Fixed(n) if size_scope => out_record.size = n,
						Fallback::Median if size_scope => {
							if let Some(m) = median {
								out_record.size = m;
							}
						},
						Fallback::Drop => {
							rep.dropped += 1;
							return;
						},
						_ => {},
					}
				},
			}
		}

		// Applied last, and never to a record still at 0: a size of 0 is the
		// marker for "never resolved", and turning it into `size_add` would
		// silently invent an object.
		if opts.size_add != 0 && out_record.size != 0 {
			out_record.size = out_record.size.saturating_add(opts.size_add);
		}

		// SET records are read for their sizes above, then dropped: in
		// `--gets-only` the replayed stream is references alone, and every
		// write the cache performs is a fill after a miss.
		if opts.gets_only && r.command != CMD_GET {
			rep.skipped_sets += 1;
			return;
		}

		out_record.encode(&mut enc);
		crc.update(&enc);
		rep.written += 1;

		if let Err(e) = out.write_all(&enc) {
			err = Some(e);
		}
	})?;

	if let Some(e) = err {
		return Err(e);
	}

	out.flush()?;

	print_report(opts, &rep, crc.finish());

	Ok(())
}

fn print_report(opts: &Opts, rep: &Report, crc: u32) {
	let pct = |n: u64, d: u64| match d {
		0 => 0.0,
		d => 100.0 * n as f64 / d as f64,
	};

	println!();
	println!("input                {}", opts.input);
	println!("output               {}", opts.output);
	println!();
	println!("records read         {}", commas(rep.records));
	println!("  GET                {}", commas(rep.gets));
	println!("  SET                {}", commas(rep.sets));
	println!();
	println!(
		"zero-size GET        {:>15}  ({:.2}% of GETs)",
		commas(rep.zero_gets),
		pct(rep.zero_gets, rep.gets),
	);
	println!(
		"zero-size SET        {:>15}  ({:.2}% of SETs)",
		commas(rep.zero_sets),
		pct(rep.zero_sets, rep.sets),
	);
	println!();
	println!("in patch scope       {}", commas(rep.in_scope));

	// Under `global` the table never moves, so there is no preceding/following
	// distinction to report -- every hit is the key's one entry.
	match opts.resolve {
		Resolve::Global => {
			let resolved = rep.from_preceding + rep.from_following;

			println!(
				"  resolved           {:>15}  ({:.2}%)",
				commas(resolved),
				pct(resolved, rep.in_scope),
			);
		},

		Resolve::Nearest => {
			println!(
				"  from preceding     {:>15}  ({:.2}%)",
				commas(rep.from_preceding),
				pct(rep.from_preceding, rep.in_scope),
			);
			println!(
				"  from following     {:>15}  ({:.2}%)",
				commas(rep.from_following),
				pct(rep.from_following, rep.in_scope),
			);
		},
	}
	println!(
		"  unresolved         {:>15}  ({:.2}%)",
		commas(rep.unresolved),
		pct(rep.unresolved, rep.in_scope),
	);

	if rep.dropped > 0 {
		println!("  dropped            {}", commas(rep.dropped));
	}

	if opts.gets_only {
		println!();
		println!("SETs read, not emitted {}", commas(rep.skipped_sets));
	}

	println!();
	println!("records written      {}", commas(rep.written));
	println!("output crc32         {crc:08x}");
}

fn commas(n: u64) -> String {
	let s = n.to_string();
	let mut out = String::with_capacity(s.len() + s.len() / 3);

	for (i, c) in s.chars().enumerate() {
		if i > 0 && (s.len() - i) % 3 == 0 {
			out.push(',');
		}

		out.push(c);
	}

	out
}

// ----------------------------------------------------------------- tests ---

#[cfg(test)]
mod tests {
	use super::*;

	fn rec(command: u8, key: u64, size: u32, ttl: u32) -> Record {
		Record { timestamp: 0, command, key, size, ttl }
	}

	#[test]
	fn a_record_survives_a_round_trip() {
		let r = rec(CMD_SET, 0xDEAD_BEEF_1234_5678, 4096, 900);
		let mut buf = [0u8; CHUNK];

		r.encode(&mut buf);
		let back = Record::decode(&buf);

		assert_eq!(back.command, r.command);
		assert_eq!(back.key, r.key);
		assert_eq!(back.size, r.size);
		assert_eq!(back.ttl, r.ttl);
	}

	#[test]
	fn the_table_overwrites_rather_than_accumulating() {
		let mut t = SizeTable::with_capacity(16);

		t.set(7, 100, 10);
		t.set(7, 200, 20);

		assert_eq!(t.get(7), Some((200, 20)));
		assert_eq!(t.len, 1);
	}

	#[test]
	fn first_keeps_the_first() {
		let mut t = SizeTable::with_capacity(16);

		t.offer(7, 100, 10, GlobalPick::First);
		t.offer(7, 200, 20, GlobalPick::First);

		assert_eq!(t.get(7), Some((100, 10)));
	}

	#[test]
	fn the_table_survives_growth() {
		let mut t = SizeTable::with_capacity(16);

		for k in 0..10_000u64 {
			t.set(k, (k as u32) + 1, k as u32);
		}

		assert_eq!(t.len, 10_000);

		for k in 0..10_000u64 {
			assert_eq!(t.get(k), Some(((k as u32) + 1, k as u32)));
		}

		assert_eq!(t.get(10_001), None);
	}

	/// Key 0 is a legitimate key and must not read as an empty slot.
	#[test]
	fn key_zero_is_storable() {
		let mut t = SizeTable::with_capacity(16);

		t.set(0, 64, 5);

		assert_eq!(t.get(0), Some((64, 5)));
		assert_eq!(t.get(1), None);
	}

	/// The two-pass rule: a zero-size record before the key's first sized
	/// record adopts that following one; after it, it adopts the most recent
	/// preceding one.
	#[test]
	fn resolution_prefers_preceding_then_falls_forward() {
		let trace = [
			rec(CMD_GET, 1, 0, 0),    // 0: no preceding size -> takes the 500
			rec(CMD_SET, 1, 500, 60), // 1
			rec(CMD_GET, 1, 0, 0),    // 2: preceding -> 500
			rec(CMD_SET, 1, 900, 70), // 3
			rec(CMD_GET, 1, 0, 0),    // 4: most recent preceding -> 900
			rec(CMD_GET, 2, 0, 0),    // 5: key 2 never sized -> unresolved
		];

		let mut table = SizeTable::with_capacity(16);

		for r in &trace {
			if r.size != 0 {
				table.offer(r.key, r.size, r.ttl, GlobalPick::First);
			}
		}

		let mut got = Vec::new();

		for r in &trace {
			match r.size {
				0 => got.push(table.get(r.key)),
				_ => table.offer(r.key, r.size, r.ttl, GlobalPick::Last),
			}
		}

		assert_eq!(got[0], Some((500, 60)), "falls forward to the first size");
		assert_eq!(got[1], Some((500, 60)), "most recent preceding");
		assert_eq!(got[2], Some((900, 70)), "the newer preceding size wins");
		assert_eq!(got[3], None, "a key with no size anywhere stays unresolved");
	}

	/// `global` freezes the table at pass 1, so a key keeps one size and one
	/// TTL for the whole trace -- the property Sari's traces have.
	#[test]
	fn global_keeps_one_value_per_key() {
		let trace = [
			rec(CMD_SET, 1, 500, 60),
			rec(CMD_GET, 1, 0, 0),
			rec(CMD_SET, 1, 900, 70),
			rec(CMD_GET, 1, 0, 0),
		];

		for (pick, want) in [
			(GlobalPick::First, (500, 60)),
			(GlobalPick::Last, (900, 70)),
		] {
			let mut table = SizeTable::with_capacity(16);

			for r in &trace {
				if r.size != 0 {
					table.offer(r.key, r.size, r.ttl, pick);
				}
			}

			// Pass 2 never writes in global mode, so both GETs read the same
			// entry regardless of where they sit relative to the writes.
			for r in trace.iter().filter(|r| r.size == 0) {
				assert_eq!(table.get(r.key), Some(want));
			}
		}
	}

	/// `max` takes the largest size the key is ever seen at, and the TTL that
	/// accompanied it -- not the largest TTL, and not the last one.
	#[test]
	fn max_takes_the_largest_size_and_its_own_ttl() {
		let mut t = SizeTable::with_capacity(16);

		for r in [
			rec(CMD_SET, 1, 500, 60),
			rec(CMD_SET, 1, 900, 70),
			rec(CMD_SET, 1, 300, 999),
		] {
			t.offer(r.key, r.size, r.ttl, GlobalPick::Max);
		}

		assert_eq!(t.get(1), Some((900, 70)));
	}

	/// The bug this caught on cluster13: with `--source any` a GET can be the
	/// largest-size donor, and every GET carries `ttl == 0`. Taking its TTL
	/// left filled records unexpirable.
	#[test]
	fn a_ttl_less_donor_never_clears_a_real_ttl() {
		for pick in [GlobalPick::First, GlobalPick::Last, GlobalPick::Max] {
			let mut t = SizeTable::with_capacity(16);

			t.offer(1, 500, 300, pick); // a SET: has a TTL
			t.offer(1, 9000, 0, pick); // a GET: bigger, but no TTL

			let (size, ttl) = t.get(1).unwrap();

			assert_eq!(ttl, 300, "TTL was cleared by a GET donor");

			let want = match pick {
				GlobalPick::First => 500,
				GlobalPick::Last | GlobalPick::Max => 9000,
			};

			assert_eq!(size, want);
		}
	}

	/// The reverse: a key first seen on a GET has no TTL until a SET supplies
	/// one, even when that SET does not win the size.
	#[test]
	fn a_later_ttl_fills_in_where_there_was_none() {
		let mut t = SizeTable::with_capacity(16);

		t.offer(1, 9000, 0, GlobalPick::Max); // GET, big, no TTL
		t.offer(1, 500, 300, GlobalPick::Max); // SET, smaller, has one

		assert_eq!(t.get(1), Some((9000, 300)));
	}

	#[test]
	fn the_median_is_over_non_zero_sizes() {
		let mut s = Sizes::new();

		for size in [10u32, 20, 30, 40, 50] {
			s.add(size);
		}

		assert_eq!(s.median(), Some(30));
	}

	#[test]
	fn the_median_handles_sizes_past_the_small_histogram() {
		let mut s = Sizes::new();

		s.add(1);
		s.add(2);
		s.add(SMALL_MAX as u32 + 10);

		assert_eq!(s.median(), Some(2));
	}

	#[test]
	fn commas_group_by_three() {
		assert_eq!(commas(0), "0");
		assert_eq!(commas(999), "999");
		assert_eq!(commas(1_000), "1,000");
		assert_eq!(commas(140_012_272), "140,012,272");
	}
}
