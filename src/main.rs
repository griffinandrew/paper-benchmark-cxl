/*
 * Copyright (c) Kia Shakiba
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
