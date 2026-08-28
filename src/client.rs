/*


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

				match self.client.get(access.key) {
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

		match self.client.get(access.key) {
			Ok(value) => {
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
			},
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



*/
//use std::sync::{Once};

//static INIT: Once = Once::new(); 


use std::{
    fmt::{self, Display},
    time::{Instant, Duration},
    error::Error,
    sync::Arc,
};

use clap::ValueEnum;
use crossbeam_channel::Receiver;

use crate::{
    access::{Access, Command},
    stats::Stats,
    cache_backend::CacheBackend,
};

pub type ClientReceiver = Receiver<ClientEvent>;

pub struct BenchmarkClient {
    // `Arc`, not `Box`: multiple `BenchmarkClient`s (one per `-c N` client
    // thread) now share one backend/cache instance, matching what `-c N`
    // is supposed to model (N concurrent clients against one cache) --
    // see `CacheBackend`'s own doc comment for why `&self` methods make
    // this safe.
    client: Arc<dyn CacheBackend>,
    events: ClientReceiver,
    stats: Stats,

    client_type: ClientType,
}

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
        backend: Arc<dyn CacheBackend>,
        auth: Option<String>,
        events: ClientReceiver,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        Self::with_expected_accesses(backend, auth, events, 0)
    }

    /// Same as [`Self::new`], but pre-sizes this client's `Stats` latency
    /// buffers for `expected_accesses` up front -- see `Stats::with_capacity`
    /// for why this matters under a strictly NUMA-bound global allocator.
    /// `0` (what `Self::new` passes) falls back to `Stats::default()`'s
    /// ordinary grow-as-you-go Vecs.
    pub fn with_expected_accesses(
        backend: Arc<dyn CacheBackend>,
        auth: Option<String>,
        events: ClientReceiver,
        expected_accesses: usize,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        if let Some(token) = &auth {
            backend.auth(token)?;
        }

        backend.wipe()?;

        let stats = if expected_accesses > 0 {
            Stats::with_capacity(expected_accesses)
        } else {
            Stats::default()
        };

        Ok(BenchmarkClient {
            client: backend,
            events,
            stats,

            client_type: ClientType::Lookaside, //this should be diff i think.....
        })
    }

    pub fn with_client_type(mut self, client_type: ClientType) -> Self {
        self.client_type = client_type;
        self
    }

    /// Caps retained latency samples for this client's `Stats`. See
    /// `Stats::with_max_samples`; `0` keeps every sample.
    pub fn with_max_latency_samples(mut self, max_samples: usize) -> Self {
        self.stats = std::mem::take(&mut self.stats).with_max_samples(max_samples);
        self
    }


    
    pub fn run(&mut self) -> Result<Stats, Box<dyn Error + Send + Sync>> {
        let max_wait = Duration::from_secs(20);

        while let Ok(event) = self.events.recv_timeout(max_wait) {
            match event {
                ClientEvent::Ping => self.handle_ping()?,
                ClientEvent::Access(access) => self.handle_access(access)?,
            }
        }

        Ok(self.stats.clone())
    }



    /*
    pub fn run(&mut self) -> Result<Stats, Box<dyn Error + Send + Sync>> {
        let max_wait = Duration::from_secs(50);

        loop {
            match self.events.recv_timeout(max_wait) {
                Ok(ClientEvent::Ping) => {
                    if let Err(e) = self.handle_ping() {
                        eprintln!("[client] ping error: {e}");
                        return Err(e);
                    }
                }
                Ok(ClientEvent::Access(access)) => {
                    if let Err(e) = self.handle_access(access) {
                        eprintln!("[client] access error: {e}");
                        return Err(e);
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    eprintln!("[client] recv timeout - exiting");
                    break;
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    break;  // normal termination
                }
            }
        }

        Ok(self.stats.clone())
    }
        */

    fn handle_ping(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let start_time = Instant::now();

        self.client.ping()?;
        self.stats.store_ping_time(start_time);

        Ok(())
    }

    fn handle_access(&mut self, access: Access) -> Result<(), Box<dyn Error + Send + Sync>> {
        match self.client_type {
            ClientType::Lookaside => self.handle_lookaside(access),
            ClientType::ReadThrough => self.handle_read_through(access),
        }
    }

    fn handle_lookaside(&mut self, access: Access) -> Result<(), Box<dyn Error + Send + Sync>> {
        match access.command {
            Command::Get => {
                let start_time = Instant::now();

                match self.client.get(access.key) {
                    Ok(Some(value)) => {
                        self.stats.store_get_time(start_time);

                        let value: Vec<u8> = value;
                            //.try_into()
                            //.map_err(|_| "Invalid value encoding")?;

                        self.stats.store_get_size(value.len() as u64);
                    },

                    // Ok(None) => cache miss; record time but no value
                    Ok(None) => {
                        self.stats.store_get_time(start_time);
                    },

                    Err(err) => {
                        return Err(err);
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

    fn handle_read_through(&mut self, access: Access) -> Result<(), Box<dyn Error + Send + Sync>> {
        if access.command != Command::Get {
            return Ok(());
        }

        // USE_GET_INTO=1 takes the per-GET allocation off the measured path by
        // copying into one reused buffer. Diagnostic only: it measures the
        // cache's read cost with the allocator's contribution removed, so the
        // difference between the two modes IS the allocation term.
        if use_get_into() {
            let get_start_time = Instant::now();
            let hit = GET_BUF.with(|b| self.client.get_into(access.key, &mut b.borrow_mut()))?;

            if hit {
                self.stats.store_get_time(get_start_time);
                // Wrapper probe: the same bracketed span the benchmark medians,
                // sampled on the backend's cadence (its tick advanced this op).
                if crate::cache_backend::wp_enabled()
                    && crate::cache_backend::WP_TICK.with(|c| c.get() & 63 == 1)
                {
                    if let Ok(mut v) = crate::cache_backend::WP_TOTAL.lock() {
                        v.push(get_start_time.elapsed().as_nanos() as u64);
                    }
                }
                let len = GET_BUF.with(|b| b.borrow().len());
                self.stats.store_get_size(len as u64);
            } else {
                let size = access.value.len() as u64;
                let set_start_time = Instant::now();
                self.client.set(access.key, access.value, access.ttl)?;
                self.stats.store_set_time(set_start_time);
                self.stats.store_set_size(size);
            }

            return Ok(());
        }

        let get_start_time = Instant::now();

        match self.client.get(access.key) {
            Ok(Some(value)) => {
                self.stats.store_get_time(get_start_time);

                let value: Vec<u8> = value;
                    //.try_into()
                    //.map_err(|_| "Invalid value encoding")?;
                //println!("Read-through get hit for key: {}, value length: {}", access.key, value.len());
                //println!("Value content (first 100 bytes or full length if shorter): {:?}", &value[..std::cmp::min(100, value.len())]);

                self.stats.store_get_size(value.len() as u64);
            },

            Ok(None) => {
                // miss -> perform set to populate backend
                let size = access.value.len() as u64;
                let set_start_time = Instant::now();

                self.client.set(access.key, access.value, access.ttl)?;

                self.stats.store_set_time(set_start_time);
                self.stats.store_set_size(size);
            },

            Err(err) => {
                return Err(err);
            },
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









thread_local! {
    /// One reusable destination buffer per client thread for the `USE_GET_INTO`
    /// path. Grows to the largest value seen and then stops allocating, which
    /// is the entire point: it removes malloc from the measured region.
    static GET_BUF: std::cell::RefCell<Vec<u8>> =
        std::cell::RefCell::new(Vec::with_capacity(1 << 20));
}

/// Whether to measure through `get_into` instead of `get`. Read once.
fn use_get_into() -> bool {
    static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLAG.get_or_init(|| std::env::var("USE_GET_INTO").map(|v| v == "1").unwrap_or(false))
}
