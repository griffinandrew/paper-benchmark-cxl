
/* 

/* Copyright (c) Kia Shakiba
 *
 * This source code is licensed under the GNU AGPLv3 license found in the
 * LICENSE file in the root directory of this source tree.
 */

mod access;
mod client;
mod stats;

use std::{
	thread,
	sync::Arc,
	io::{self, Seek, SeekFrom},
	path::{Path, PathBuf},
	time::Duration,
};

use clap::Parser;
use crossbeam_channel::bounded;

use kwik::{
	fmt,
	file::{
		FileReader,
		binary::{BinaryReader, SizedChunk},
	},
	progress::{Progress, Tag},
};

use crate::{
	client::{BenchmarkClient, ClientType, ClientEvent},
	access::Access,
	stats::Stats,
};

const PING_TEST_COUNT: u64 = 1_000_000;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
	#[arg(long, default_value = "127.0.0.1")]
	host: String,

	#[arg(long, default_value_t = 3145)]
	port: u32,

	#[arg(short, long)]
	auth: Option<String>,

	#[arg(short, long)]
	trace_path: Option<PathBuf>,

	//try with 1 client
	#[arg(short, long, default_value_t = 1)] //try with only 1..... ??? 
	clients: u32,

	#[arg(short, long)]
	native_time: bool,

	#[arg(long, default_value_t = ClientType::Lookaside)]
	client_type: ClientType,

	#[arg(long)]
	output_csv: Option<PathBuf>,

	#[arg(long)]
	output_plot: Option<PathBuf>,
}

fn main() {
	let args = Args::parse();

	assert!(args.clients > 0);

	let paper_addr = format!("paper://{}:{}", args.host, args.port);
	let paper_addr = Arc::new(paper_addr);

	let (sender, receiver) = bounded::<ClientEvent>(args.clients as usize);

	println!("Client type: {}", args.client_type);
	println!("Initializing {} client(s)", args.clients);

	let clients = (0..args.clients)
		.map(|_| {
			let paper_addr = paper_addr.clone();
			let receiver = receiver.clone();

			BenchmarkClient::new(&paper_addr, args.auth.clone(), receiver)
				.expect("Could not create client.")
				.with_client_type(args.client_type)
		})
		.collect::<Vec<BenchmarkClient>>();

	let tasks = clients
		.into_iter()
		.map(|mut client| thread::spawn(move || client.run()))
		.collect::<Vec<_>>();

	if args.trace_path.is_none() {
		println!("\nPerforming {} pings", fmt::number(PING_TEST_COUNT));

		let mut progress = Progress::new(PING_TEST_COUNT)
			.with_tag(Tag::Tps)
			.with_tag(Tag::Eta)
			.with_tag(Tag::Time);

		for _ in 0..PING_TEST_COUNT {
			sender.send(ClientEvent::Ping)
				.expect("Could not send ping to client.");

			progress.tick(1);
		}
	}

	if let Some(trace_path) = &args.trace_path {
		if args.native_time {
			let timespan = get_trace_timespan(trace_path)
				.expect("Invalid trace path.");

			println!("\nUsing native access time.");
			println!("Total trace timestamp: {}", fmt::timespan(timespan));
		}

		let reader = BinaryReader::<Access>::from_path(trace_path)
			.expect("Invalid trace path.");

		println!("\nProcessing {} accesses", fmt::number(reader.size() / Access::chunk_size() as u64));

		let mut progress = Progress::new(reader.size())
			.with_tag(Tag::Tps)
			.with_tag(Tag::Eta)
			.with_tag(Tag::Time);

		let mut prev_access_timestamp: Option<u64> = None;

		for mut access in reader {
			if args.native_time {
				let prev_timestamp = prev_access_timestamp.unwrap_or(access.timestamp);

				if prev_timestamp > access.timestamp {
					panic!("Invalid timestamp order.");
				}

				let sleep_duration = Duration::from_millis(access.timestamp - prev_timestamp);
				spin_sleep::sleep(sleep_duration);

				prev_access_timestamp = Some(access.timestamp);
			} else {
				access.ttl = None;
			}

			sender.send(ClientEvent::Access(access))
				.expect("Could not send access to client.");

			progress.tick(Access::chunk_size());
		}
	}

	drop(sender);

	let mut stats = Stats::default();

	for task in tasks {
		stats += task
			.join()
			.expect("Could not terminate client")
			.expect("Error executing client requests");
	}

	stats.print_ping_stats();
	stats.print_get_stats();
	stats.print_set_stats();

	if args.output_csv.is_some() || args.output_plot.is_some() {
		println!();
	}

	if let Some(path) = &args.output_csv {
		stats.save_latency_percentiles(path)
			.expect("Could not save latency percentiles.");

		println!("Saved CSV to <{}>.", path.to_str().unwrap_or(""));
	}

	if let Some(path) = &args.output_plot {
		stats.save_latency_plot(path)
			.expect("Could not save latency plot.");

		println!("Saved plot to <{}>.", path.to_str().unwrap_or(""));
	}
}

fn get_trace_timespan<P>(path: P) -> io::Result<u64>
where
	P: AsRef<Path>,
{
	let mut reader = BinaryReader::<Access>::from_path(path)?;
	let first_access = reader.read_chunk()?;

	reader.seek(SeekFrom::End(-(Access::chunk_size() as i64)))?;
	let last_access = reader.read_chunk()?;

	if last_access.timestamp < first_access.timestamp {
		panic!("Invalid timestamp order.");
	}

	Ok(last_access.timestamp - first_access.timestamp)
}








mod access;
mod client;
mod stats;
mod cache_backend;

use std::{
    thread,
    sync::Arc,
    io::{self, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::Duration,
};

use clap::Parser;
use crossbeam_channel::bounded;

use kwik::{
    fmt,
    file::{
        FileReader,
        binary::{BinaryReader, SizedChunk},
    },
    progress::{Progress, Tag},
};

use crate::{
    client::{BenchmarkClient, ClientType, ClientEvent},
    access::Access,
    stats::Stats,
    cache_backend::{PaperClientBackend, PaperCacheBackend, CacheBackend},
};

const PING_TEST_COUNT: u64 = 1_000_000;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 3145)]
    port: u32,

    #[arg(short, long)]
    auth: Option<String>,

    #[arg(short, long)]
    trace_path: Option<PathBuf>,

    #[arg(short, long, default_value_t = 4)]
    clients: u32,

    #[arg(short, long)]
    native_time: bool,

    #[arg(long, default_value_t = ClientType::Lookaside)]
    client_type: ClientType,

    #[arg(long)]
    output_csv: Option<PathBuf>,

    #[arg(long)]
    output_plot: Option<PathBuf>,

    /// Use the in-process paper_cache implementation rather than remote paper server
    #[arg(long)]
    use_cache: bool,

    /// Max cache size in bytes for in-process cache (only used when --use-cache is set)
    #[arg(long, default_value_t = 1_000_000_000u64)]
    cache_max_size: u64,
}

fn main() {
    let args = Args::parse();

    assert!(args.clients > 0);

    let paper_addr = format!("paper://{}:{}", args.host, args.port);
    let paper_addr = Arc::new(paper_addr);

    let (sender, receiver) = bounded::<ClientEvent>(args.clients as usize);

    println!("Client type: {}", args.client_type);
    println!("Initializing {} client(s)", args.clients);

    let clients = (0..args.clients)
        .map(|_| {
            let paper_addr = Arc::clone(&paper_addr);
            let receiver = receiver.clone();

            // Build and supply the backend object
            let backend: Box<dyn CacheBackend> = if args.use_cache {
                Box::new(
                    PaperCacheBackend::new(args.cache_max_size)
                        .expect("Could not create PaperCacheBackend"),
                )
            } else {
                Box::new(
                    PaperClientBackend::new(&paper_addr)
                        .expect("Could not create PaperClientBackend"),
                )
            };

            BenchmarkClient::new(backend, args.auth.clone(), receiver)
                .expect("Could not create client.")
                .with_client_type(args.client_type)
        })
        .collect::<Vec<BenchmarkClient>>();

    let tasks = clients
        .into_iter()
        .map(|mut client| thread::spawn(move || client.run()))
        .collect::<Vec<_>>();

    if args.trace_path.is_none() {
        println!("\nPerforming {} pings", fmt::number(PING_TEST_COUNT));

        let mut progress = Progress::new(PING_TEST_COUNT)
            .with_tag(Tag::Tps)
            .with_tag(Tag::Eta)
            .with_tag(Tag::Time);

        for _ in 0..PING_TEST_COUNT {
            sender.send(ClientEvent::Ping)
                .expect("Could not send ping to client.");

            progress.tick(1);
        }
    }

    if let Some(trace_path) = &args.trace_path {
        if args.native_time {
            let timespan = get_trace_timespan(trace_path)
                .expect("Invalid trace path.");

            println!("\nUsing native access time.");
            println!("Total trace timestamp: {}", fmt::timespan(timespan));
        }

        let reader = BinaryReader::<Access>::from_path(trace_path)
            .expect("Invalid trace path.");

        println!("\nProcessing {} accesses", fmt::number(reader.size() / Access::chunk_size() as u64));

        let mut progress = Progress::new(reader.size())
            .with_tag(Tag::Tps)
            .with_tag(Tag::Eta)
            .with_tag(Tag::Time);

        let mut prev_access_timestamp: Option<u64> = None;

        for mut access in reader {
            if args.native_time {
                let prev_timestamp = prev_access_timestamp.unwrap_or(access.timestamp);

                if prev_timestamp > access.timestamp {
                    panic!("Invalid timestamp order.");
                }

                let sleep_duration = Duration::from_millis(access.timestamp - prev_timestamp);
                spin_sleep::sleep(sleep_duration);

                prev_access_timestamp = Some(access.timestamp);
            } else {
                access.ttl = None;
            }

            sender.send(ClientEvent::Access(access))
                .expect("Could not send access to client.");

            progress.tick(Access::chunk_size());
        }
    }

    drop(sender);

    let mut stats = Stats::default();

    for task in tasks {
        stats += task
            .join()
            .expect("Could not terminate client")
            .expect("Error executing client requests");
    }

    stats.print_ping_stats();
    stats.print_get_stats();
    stats.print_set_stats();

    if args.output_csv.is_some() || args.output_plot.is_some() {
        println!();
    }

    if let Some(path) = &args.output_csv {
        stats.save_latency_percentiles(path)
            .expect("Could not save latency percentiles.");

        println!("Saved CSV to <{}>.", path.to_str().unwrap_or(""));
    }

    if let Some(path) = &args.output_plot {
        stats.save_latency_plot(path)
            .expect("Could not save latency plot.");

        println!("Saved plot to <{}>.", path.to_str().unwrap_or(""));
    }
}

fn get_trace_timespan<P>(path: P) -> io::Result<u64>
where
    P: AsRef<Path>,
{
    let mut reader = BinaryReader::<Access>::from_path(path)?;
    let first_access = reader.read_chunk()?;

    reader.seek(SeekFrom::End(-(Access::chunk_size() as i64)))?;
    let last_access = reader.read_chunk()?;

    if last_access.timestamp < first_access.timestamp {
        panic!("Invalid timestamp order.");
    }

    Ok(last_access.timestamp - first_access.timestamp)
}




*/


//#![cfg_attr(any(feature = "allocator_api", feature = value_dram, feature(allocator_api), feature(clone_from_ref)))]

#![cfg_attr(
    any(feature = "allocator_api", feature = "value_dram"), // 1. The Condition
    feature(allocator_api, clone_from_ref)      // 2. The Attribute to apply
)]
mod access;
mod client;
mod stats;
mod cache_backend;

//use paper_cache::allocator::HybridObjects;

//#[cfg(not(feature = "pmem_region_alloc"))]
//#[global_allocator]
//static GLOBAL: paper_cache::allocator::HybridObjects = paper_cache::allocator::HybridObjects;
//static GLOBAL: paper_cache::allocator::RegionHybrid = paper_cache::allocator::RegionHybrid;

//#[global_allocator]
//static GLOBAL: paper_cache::allocator::HybridObjects = paper_cache::allocator::HybridObjects;



use std::{
    thread,
    io::{self, Seek, SeekFrom, BufRead, Write},
    path::{Path, PathBuf},
    time::Duration,
    sync::Arc,
};


use clap::Parser;
use crossbeam_channel::bounded;

use kwik::{
    fmt,
    file::{
        FileReader,
        binary::{BinaryReader, SizedChunk},
    },
    progress::{Progress, Tag},
};

use crate::{
    client::{BenchmarkClient, ClientType, ClientEvent},
    access::Access,
    stats::Stats,
    cache_backend::{PaperCacheBackend, CacheBackend},
};

const PING_TEST_COUNT: u64 = 1_000_000;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    // kept for compatibility but they are not used when running in-process cache
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 3145)]
    port: u32,

    #[arg(short, long)]
    auth: Option<String>,

    #[arg(short, long)]
    trace_path: Option<PathBuf>,

    #[arg(short, long, default_value_t = 1)]
    clients: u32,

    #[arg(short, long)]
    native_time: bool,

    #[arg(long, default_value_t = ClientType::ReadThrough)]
    client_type: ClientType,

    #[arg(long)]
    output_csv: Option<PathBuf>,

    #[arg(long)]
    output_plot: Option<PathBuf>,

    /// Use the in-process paper_cache implementation (default: true)
    #[arg(long, default_value_t = true)]
    use_cache: bool,

    /// Max cache size in bytes for in-process cache (only used when --use-cache is set)
    //#[arg(long, default_value_t = 25_769_803_776u64)] //17_179_869_184 B      25_769_803_776  //over allocate to 25gb... 

    #[arg(long, default_value_t = 25_769_803_776u64)]
    //#[arg(long, default_value_t = 34_359_738_368u64)]
    cache_max_size: u64,
}


fn prewarm(bytes: usize) {
    let mut v: Vec<u8> = vec![0u8; bytes];  // vec! zeros, which touches every byte
    // Optional but cheap: explicit per-page touch to be sure even if a future
    // allocator quirk skips zeroing.
    let page = 4096usize;
    let mut i = 0;
    while i < bytes {
        unsafe { std::ptr::write_volatile(v.as_mut_ptr().add(i), 0u8); }
        i += page;
    }
    drop(v);  // returns to jemalloc's pool; decay-off keeps it resident
}


#[cfg(feature = "devdax_bump")]
fn init_storage() {
    paper_cache::allocator::DevDaxBump::init();
}



fn prefault_fast_tier(buf: &mut [u8]) {
    let ptr = buf.as_mut_ptr() as *mut libc::c_void;
    let len = buf.len();

    // Safety check: madvise requires page-aligned pointers.
    // Vector allocations from jemalloc over 4KB are automatically page-aligned,
    // but this guard ensures the kernel won't reject the call with EINVAL.
    if (ptr as usize) % 4096 != 0 {
        eprintln!("Warning: Buffer is not page-aligned. Falling back to safe manual touch.");
        for page in buf.chunks_mut(4096) {
            page[0] = 0;
        }
        return;
    }

    unsafe {
        // Modern Linux kernels (5.14+) - populates page tables entirely in kernel space
        let mut ret = libc::madvise(ptr, len, libc::MADV_POPULATE_WRITE);
        
        // Fallback for older kernels if MADV_POPULATE_WRITE isn't supported
        if ret != 0 {
            ret = libc::madvise(ptr, len, libc::MADV_WILLNEED);
        }

        if ret != 0 {
            panic!("madvise failed to prefault memory. Error code: {}", ret);
        }
    }
}


fn main() {
    #[cfg(feature = "pmem_region_alloc")] { paper_cache::allocator::RegionHybrid::init();}



    #[cfg(feature = "umf")]
    paper_cache::allocator::HybridObjects::init_and_prewarm(
        1,                                    // PMEM node
        30 * 1024 * 1024 * 1024,               // 48 GiB working set
    );

    #[cfg(feature = "value_dram")]
    paper_cache::allocator::ValueDRAM::init_and_prewarm(2, 8 * 1024 * 1024 * 1024); // 8 GiB working set

    #[cfg(feature = "daxpmem")] {
        paper_cache::allocator::DAXPMEM::init_and_prewarm();
    }

    //println!("warmup done (pid {}), attach perf then press enter... ", std::process::id());
    //io::stderr().flush().unwrap();
    //let mut line = String::new();
    //io::stdin().lock().read_line(&mut line).unwrap();
    
    #[cfg(feature = "devdax_bump")] { init_storage(); }

    //#[cfg(not(feature = "allocator_api"))] {
        // Pre-warm the allocator with a large allocation to ensure that the memory is resident and ready for use.
        // This can help reduce latency spikes during the benchmark caused by on-demand memory allocation.
        //prewarm(25_769_803_776); // Pre-warm with 25 GB
    //}

    //let tsc_hz = paper_cache::calibrate_tsc_hz();
    //println!("Starting benchmark...");

    // Prefault disabled: this unconditionally touched 25 GiB up front on every
    // all_dram-feature run regardless of the actual configured cache size,
    // which was skewing SET-latency comparisons (allocations were drawing
    // from an already-resident pool instead of paying real first-touch page
    // fault cost). No config should be prefaulting.
    /*
    #[cfg(feature = "all_dram")] {
        let mut buf = vec![0u8; 25 * 1024 * 1024 * 1024];
        prefault_fast_tier(&mut buf);
    }
    */



    let args = Args::parse();

    assert!(args.clients > 0);

    let (sender, receiver) = bounded::<ClientEvent>(args.clients as usize);

   // let (sender, receiver) = bounded::<ClientEvent>(2 as usize);

    println!("Client type: {}", args.client_type);
    println!("Initializing {} client(s)", args.clients);

    // One shared cache for every client -- `-c N` is meant to model N
    // concurrent clients against a single cache (matching real deployment
    // via paper-server), not N independent caches. Building a separate
    // `PaperCacheBackend` per client (the previous behavior) silently
    // multiplied the real memory ceiling by N: with `-c 8` and
    // `--cache-max-size 15G`, the true aggregate ceiling was up to 120G
    // (8 independent 15G-capable caches), not 15G, which is what caused
    // real OOM kills under `-c 8` that never happened under `-c 1`. Safe to
    // share now that `CacheBackend`'s methods take `&self` (PaperCache's
    // own methods already do, and are designed for concurrent shared use).
    // `--use-cache` has no real effect either way here (see the comment
    // below) -- both branches already built the same in-process cache.
    let backend: Arc<dyn CacheBackend> = if args.use_cache {
        Arc::new(
            PaperCacheBackend::new(args.cache_max_size)
                .expect("Could not create PaperCacheBackend"),
        )
    } else {
        // If someone explicitly disables --use-cache, we still create the in-process cache to avoid networking;
        // alternatively we could error here. For now: create the in-process cache anyway.
        Arc::new(
            PaperCacheBackend::new(args.cache_max_size)
                .expect("Could not create PaperCacheBackend"),
        )
    };

    // Pre-size each client's `Stats` latency buffers for roughly its share
    // of the trace, instead of letting them grow via `Vec::push`'s
    // amortized doubling across the whole run. `reader.size()` is already
    // read again below to print "Processing N accesses" before the run
    // starts -- reading it once more here (cheap: `BinaryReader` just
    // stats/opens the file to get its record count, doesn't read the
    // records themselves) lets that same number size these buffers up
    // front, while node 0 is still fresh, rather than paying dozens of
    // reallocations -- the last and largest of which lands at the worst
    // possible time, when node 0 is most fragmented -- spread across the
    // run. See `Stats::with_capacity`'s own doc comment for the full
    // reasoning and the real allocation-failure abort this fixes.
    let expected_total_accesses: usize = args.trace_path.as_ref()
        .and_then(|path| BinaryReader::<Access>::from_path(path).ok())
        .map(|reader| (reader.size() / Access::chunk_size() as u64) as usize)
        .unwrap_or(0);
    let expected_accesses_per_client: usize =
        expected_total_accesses / args.clients.max(1) as usize;

    // Same reasoning as the per-client buffers above, applied to the final
    // cross-client accumulator: the post-run merge loop below
    // (`stats += task.join()...`) used to rebuild `stats`'s Vecs from
    // scratch on every single client merge, so the *last* merge needed one
    // huge contiguous allocation sized for the complete combined dataset --
    // confirmed directly, this crashed a real `-c 4` run against the full
    // standard_web.bin trace with a single ~224 MB allocation failure right
    // after the trace itself had already processed cleanly to 100%.
    // Pre-sizing `stats` for the *whole* trace up front, before the run
    // even starts (i.e. before node 0 has had a chance to fragment at all),
    // means that allocation happens here instead, at the best possible
    // time -- see `Stats::with_capacity`/`AddAssign`'s own doc comments.
    let mut stats = Stats::with_capacity(expected_total_accesses);

    let clients = (0..args.clients)
        .map(|_| {
            let receiver = receiver.clone();

            BenchmarkClient::with_expected_accesses(
                Arc::clone(&backend),
                args.auth.clone(),
                receiver,
                expected_accesses_per_client,
            )
                .expect("Could not create client.")
                .with_client_type(args.client_type)
        })
        .collect::<Vec<BenchmarkClient>>();

    drop(receiver); // drop the original receiver in the main thread since clients have their own clones

    let tasks = clients
        .into_iter()
        .map(|mut client| thread::spawn(move || client.run()))
        .collect::<Vec<_>>();

    if args.trace_path.is_none() {
        println!("\nPerforming {} pings", fmt::number(PING_TEST_COUNT));

        let mut progress = Progress::new(PING_TEST_COUNT)
            .with_tag(Tag::Tps)
            .with_tag(Tag::Eta)
            .with_tag(Tag::Time);

        for _ in 0..PING_TEST_COUNT {
            sender.send(ClientEvent::Ping)
                .expect("Could not send ping to client.");

            progress.tick(1);
        }
    }

    if let Some(trace_path) = &args.trace_path {
        if args.native_time {
            let timespan = get_trace_timespan(trace_path)
                .expect("Invalid trace path.");

            println!("\nUsing native access time.");
            println!("Total trace timestamp: {}", fmt::timespan(timespan));
        }

        let reader = BinaryReader::<Access>::from_path(trace_path)
            .expect("Invalid trace path.");

        println!("\nProcessing {} accesses", fmt::number(reader.size() / Access::chunk_size() as u64));

        let mut progress = Progress::new(reader.size())
            .with_tag(Tag::Tps)
            .with_tag(Tag::Eta)
            .with_tag(Tag::Time);

        let mut prev_access_timestamp: Option<u64> = None;

        for mut access in reader {
            if args.native_time {
                let prev_timestamp = prev_access_timestamp.unwrap_or(access.timestamp);

                if prev_timestamp > access.timestamp {
                    panic!("Invalid timestamp order.");
                }

                let sleep_duration = Duration::from_millis(access.timestamp - prev_timestamp);
                spin_sleep::sleep(sleep_duration);

                prev_access_timestamp = Some(access.timestamp);
            } else {
                access.ttl = None;
            }

            sender.send(ClientEvent::Access(access))
                .expect("Could not send access to client.");

            progress.tick(Access::chunk_size());
        }
    }

    //drop(sender);

    for task in tasks {
        stats += task
            .join()
            .expect("Could not terminate client")
            .expect("Error executing client requests");
    }

    // `AddAssign` no longer sorts on every merge (see its own doc comment)
    // -- restore chronological order once here, which is all
    // `save_latency_plot` below actually needs it for.
    stats.sort_by_time();

    stats.print_ping_stats();
    stats.print_get_stats();
    stats.print_set_stats();
    //CacheBackend::report_stats_lru(backend).expect("Could not report LRU stats.");

    if args.output_csv.is_some() || args.output_plot.is_some() {
        println!();
    }

    if let Some(path) = &args.output_csv {
        stats.save_latency_percentiles(path)
            .expect("Could not save latency percentiles.");

        println!("Saved CSV to <{}>.", path.to_str().unwrap_or(""));
    }

    if let Some(path) = &args.output_plot {
        stats.save_latency_plot(path)
            .expect("Could not save latency plot.");

        println!("Saved plot to <{}>.", path.to_str().unwrap_or(""));
    }

    //paper_cache::report_set(tsc_hz);
    //paper_cache::report_get(tsc_hz);
}

fn get_trace_timespan<P>(path: P) -> io::Result<u64>
where
    P: AsRef<Path>,
{
    let mut reader = BinaryReader::<Access>::from_path(path)?;
    let first_access = reader.read_chunk()?;

    reader.seek(SeekFrom::End(-(Access::chunk_size() as i64)))?;
    let last_access = reader.read_chunk()?;

    if last_access.timestamp < first_access.timestamp {
        panic!("Invalid timestamp order.");
    }

    Ok(last_access.timestamp - first_access.timestamp)
}



