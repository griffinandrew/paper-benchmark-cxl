/*
 * Copyright (c) Kia Shakiba
 *
 * This source code is licensed under the GNU AGPLv3 license found in the
 * LICENSE file in the root directory of this source tree.
 */

use std::{
	fmt::{self, Display},
	time::{Instant, Duration},
};

use clap::ValueEnum;
use crossbeam_channel::Receiver;
use paper_client::{PaperClient, PaperClientError};

use crate::{
	access::{Access, Command},
	stats::Stats,
};

pub type ClientReceiver = Receiver<ClientEvent>;

pub struct BenchmarkClient {
	client: PaperClient,
	events: ClientReceiver,
	stats: Stats,

	client_type: ClientType,
}

//counters for sets and gets in traces
#[cfg(debug_assertions)] static mut NUM_GETS: u64 = 0;
#[cfg(debug_assertions)] static mut NUM_SETS: u64 = 0;
#[cfg(debug_assertions)] static PRINT_THRESHOLD: u64 = 100000;
#[cfg(debug_assertions)] static mut NUM_REQS: u64 = 0;


#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum ClientType {
	Lookaside,
	ReadThrough,
}

pub enum ClientEvent {
	Ping,
	Access(Access),
}

impl BenchmarkClient {
	pub fn new(
		paper_addr: &str,
		auth: Option<String>,
		events: ClientReceiver,
	) -> Result<Self, PaperClientError> {
		let mut client = PaperClient::new(paper_addr)?;

		if let Some(token) = &auth {
			client.auth(token)?;
		}

		client.wipe()?;

		let benchmark_client = BenchmarkClient {
			client,
			events,
			stats: Stats::default(),

			client_type: ClientType::Lookaside,
		};

		Ok(benchmark_client)
	}

	pub fn with_client_type(mut self, client_type: ClientType) -> Self {
		self.client_type = client_type;
		self
	}

	pub fn run(&mut self) -> Result<Stats, PaperClientError> {
		let max_wait = Duration::from_secs(5);

		while let Ok(event) = self.events.recv_timeout(max_wait) {
			match event {
				ClientEvent::Ping => self.handle_ping()?,
				ClientEvent::Access(access) => self.handle_access(access)?,
			}
		}

		Ok(self.stats.clone())
	}

	fn handle_ping(&mut self) -> Result<(), PaperClientError> {
		let start_time = Instant::now();

		self.client.ping()?;
		self.stats.store_ping_time(start_time);

		Ok(())
	}

	fn handle_access(&mut self, access: Access) -> Result<(), PaperClientError> {
		match self.client_type {
			ClientType::Lookaside => self.handle_lookaside(access),
			ClientType::ReadThrough => self.handle_read_through(access),
		}
	}

	fn handle_lookaside(&mut self, access: Access) -> Result<(), PaperClientError> {
		match access.command {
			Command::Get => {
				let start_time = Instant::now();

				match self.client.get(&access.key) {
					Ok(value) => {
						self.stats.store_get_time(start_time);

						let value: &str = (&value)
							.try_into()
							.map_err(|_| PaperClientError::Internal)?;

						self.stats.store_get_size(value.len() as u64);
					},

					Err(err) if !matches!(err, PaperClientError::CacheError(_)) => {
						return Err(err);
					},

					Err(_) => {
						self.stats.store_get_time(start_time);
					},
				}
			},

			Command::Set => {
				let size = access.value.len() as u64;
				let start_time = Instant::now();

				self.client.set(access.key, access.value, access.ttl)?;

				self.stats.store_set_time(start_time);
				self.stats.store_set_size(size);
			},
		}
		Ok(())
	}

	fn handle_read_through(&mut self, access: Access) -> Result<(), PaperClientError> {
		if access.command != Command::Get {
			return Ok(());
		}

		let get_start_time = Instant::now();

		match self.client.get(&access.key) {
			Ok(value) => {
				#[cfg(debug_assertions)] {unsafe { NUM_GETS += 1; }}
				self.stats.store_get_time(get_start_time);

				let value: &str = (&value)
					.try_into()
					.map_err(|_| PaperClientError::Internal)?;

				self.stats.store_get_size(value.len() as u64);
			},

			Err(err) if !matches!(err, PaperClientError::CacheError(_)) => {
				return Err(err);
			},

			Err(_) => {
				let size = access.value.len() as u64;
				let set_start_time = Instant::now();
				self.client.set(access.key, access.value, access.ttl)?;
				self.stats.store_set_time(set_start_time);
				self.stats.store_set_size(size);
				#[cfg(debug_assertions)] {unsafe { NUM_SETS += 1; }}
			},
		}

		#[cfg(debug_assertions)] {
			unsafe {
				NUM_REQS += 1;
				if NUM_REQS % PRINT_THRESHOLD == 0 {
					let total = NUM_GETS + NUM_SETS;
					let num_get = NUM_GETS;
					let num_set = NUM_SETS;
					println!("Processed {} requests ({} gets, {} sets)", total, num_get, num_set);
					println!("Get ratio: {:.2}%", (num_get as f64 / total as f64) * 100.0);
					println!("Set ratio: {:.2}%", (num_set as f64 / total as f64) * 100.0);
				}
			}
		}

		Ok(())
	}
}

impl Display for ClientType {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let s = match self {
			ClientType::Lookaside => "lookaside",
			ClientType::ReadThrough => "read-through",
		};

		write!(f, "{s}")
	}
}
