/*
// New cache backend abstraction + adapters for paper_client and paper_cache
use std::error::Error;

//use paper_client::{PaperClient, PaperClientError};
use paper_cache::{PaperCache, PaperPolicy, CacheError as PcError};

/// Trait that abstracts a cache backend for the benchmark.
/// - get returns Ok(Some(Vec<u8>)) for a cache hit,
///   Ok(None) for a cache miss,
///   Err(..) for fatal/backend errors.
pub trait CacheBackend: Send {
    fn ping(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>, Box<dyn Error + Send + Sync>>;
    fn set(&mut self, key: String, value: Vec<u8>, ttl: Option<u32>) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn wipe(&mut self) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn auth(&mut self, token: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
}

/// Adapter around the existing paper_client to expose CacheBackend.
pub struct PaperClientBackend {
    //inner: PaperClient,
    inner: (should be like paper cache.....)
}

impl PaperClientBackend {
    pub fn new(addr: &str) -> Result<Self, Box<dyn Error + Send + Sync>> {
        //let client = PaperClient::new(addr).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;


        Ok(PaperClientBackend { inner: client })

    }
}

impl CacheBackend for PaperClientBackend {
    fn ping(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // Map any successful ping result to ()
        self.inner
            .ping()
            .map(|_| ())
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>, Box<dyn Error + Send + Sync>> {
        match self.inner.get(key) {
            Ok(v) => {
                // Try to convert returned value to bytes.
                // Most value types used by clients (Vec<u8>, bytes::Bytes, etc.) support as_ref() -> &[u8].
                // This clones the bytes into a Vec<u8>.
                let bytes = v;
                Ok(Some(bytes.to_vec()))
            }
            Err(err) => match err {
                PaperClientError::CacheError(_) => Ok(None),
                other => Err(Box::new(other) as Box<dyn Error + Send + Sync>),
            },
        }
    }

    fn set(&mut self, key: String, value: Vec<u8>, ttl: Option<u32>) -> Result<(), Box<dyn Error + Send + Sync>> {
        // paper_client::set accepts Box<[u8]> in the original benchmark; convert Vec to boxed slice.
        self.inner
            .set(key, value.into_boxed_slice(), ttl)
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    fn wipe(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.inner.wipe().map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    fn auth(&mut self, token: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.inner.auth(token).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }
}

/// Adapter around your in-process paper_cache library.
/// Uses PaperCache<u64, Box<[u8]>> internally and converts to/from Vec<u8> for the trait.
pub struct PaperCacheBackend {
    inner: PaperCache<u64, Box<[u8]>>,
}

impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // default to a single LFU policy; adjust if you want CLI policy selection
        let cache = PaperCache::<u64, Box<[u8]>>::new(max_size, &[PaperPolicy::Lfu], PaperPolicy::Lfu)
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

impl CacheBackend for PaperCacheBackend {
    fn ping(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // in-process cache: no-op
        Ok(())
    }

    fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>, Box<dyn Error + Send + Sync>> {
        let key_u64 = key.parse::<u64>().map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        match self.inner.get(&key_u64) {
            Ok(boxed) => {
                // boxed is Box<[u8]>; convert to Vec<u8> and return
                Ok(Some(boxed))
            }
            Err(err) => match err {
                PcError::KeyNotFound => Ok(None),
                other => Err(Box::new(other) as Box<dyn Error + Send + Sync>),
            },
        }
    }

    fn set(&mut self, key: String, value: Vec<u8>, ttl: Option<u32>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let key_u64 = key.parse::<u64>().map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        let boxed: Box<[u8]> = value.into_boxed_slice();
        self.inner.set(key_u64, &boxed, ttl).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    fn wipe(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.inner.wipe().map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    fn auth(&mut self, _token: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        // no auth for in-process cache
        Ok(())
    }
}




*/



use std::error::Error;

use paper_cache::{PaperCache, PaperPolicy, CacheError as PcError};


#[cfg(feature = "allocator_api")]
use paper_cache::BufferPMEM;

#[cfg(feature = "value_dram")]
use paper_cache::allocator::ValueDRAM;

#[cfg(any(feature = "hybrid", feature = "hybrid_lfu", feature = "hybrid_2q", feature = "hybrid_2q_fast_admission", feature = "hybrid_fifo", feature = "hybrid_lru_sized", feature = "hybrid_s3_fifo", feature = "hybrid_2q_ghost", feature = "hybrid_s3_fifo_ghost", feature = "hybrid_s3_fifo_ghost_lazy_demotion", feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission", feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission_midpoint", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_midpoint_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_split_slow_reprieve"))]
use paper_cache::{TieredBuffer, CacheTierSize};


/// Trait that abstracts a cache backend for the benchmark.
/// - get returns Ok(Some(Vec<u8>)) for a cache hit,
///   Ok(None) for a cache miss,
///   Err(..) for fatal/backend errors.
// `&self`, not `&mut self`: `PaperCacheBackend` (the only real implementor)
// just delegates to `PaperCache`'s own methods, which are already `&self`
// and safe to call concurrently from multiple threads (DashMap-backed,
// `unsafe impl Send + Sync` in paper-cache itself). `&self` here is what
// lets multiple `BenchmarkClient`s share one backend via `Arc` instead of
// each getting its own private `PaperCacheBackend`/`PaperCache` instance
// (previously, `-c N` created N independent caches, each with its own
// worker threads and independently-enforced `cache_max_size` -- the
// aggregate memory ceiling was silently `N * cache_max_size`, not
// `cache_max_size`, which is not what `-c N` is supposed to model).
// `Sync` (not just `Send`) is required for the same reason: `Arc<dyn
// CacheBackend>` is only `Send`/`Sync` itself if the trait object is.
pub trait CacheBackend: Send + Sync {
    fn ping(&self) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Box<dyn Error + Send + Sync>>;
    fn set(&self, key: String, value: Box<[u8]>, ttl: Option<u32>) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn wipe(&self) -> Result<(), Box<dyn Error + Send + Sync>>;
    fn auth(&self, token: &str) -> Result<(), Box<dyn Error + Send + Sync>>;

    /// End-of-run snapshot of the cache's own view of itself: policy, size
    /// accounting, miss ratio, and — on a hybrid build — the tier-movement
    /// totals and live tier gauges. `None` only if the cache could not
    /// produce a status snapshot at all.
    ///
    /// Deliberately returns benchmark-local structs rather than
    /// `paper_cache::HybridStats`: that type only exists under a hybrid
    /// feature, so naming it in the trait would force every caller
    /// (`main.rs`, `stats.rs`) to carry the same `#[cfg]` cascade. The
    /// 15-way cascade over per-design accessor names lives once, inside
    /// paper-cache (`AtomicStatus::hybrid_stats`), and this reads through
    /// it — so a design added there needs no change here.
    fn cache_report(&self) -> Option<CacheReport>;
}

/// What the cache reports about itself at the end of a run. One row's worth
/// of the summary CSV, minus the client-side latency numbers (which come
/// from `Stats`, not the cache).
#[derive(Debug, Default, Clone)]
pub struct CacheReport {
    /// The active eviction policy's string form (e.g. `lru-hybrid`,
    /// `s3-fifo-lazy-demotion-reprieve-hybrid-0.1`) — this is what
    /// identifies which design produced the row, read from the cache itself
    /// rather than reconstructed from a `#[cfg]` cascade here.
    pub policy: String,

    pub max_size: u64,
    pub used_size: u64,
    pub num_objects: u64,
    pub rss: u64,
    pub hwm: u64,
    pub total_gets: u64,
    pub total_sets: u64,
    pub miss_ratio: f64,

    /// Configured fast-tier byte budget; `None` on a non-hybrid build.
    pub fast_tier_bytes: Option<u64>,

    /// `None` on a non-hybrid (single-tier) build, where tier movement
    /// doesn't exist as a concept.
    pub hybrid: Option<HybridStatsSnapshot>,
}

/// Snapshot of `paper_cache::HybridStats`, decoupled from the hybrid
/// features so the `CacheBackend` trait signature stays unconditional.
#[derive(Debug, Default, Clone, Copy)]
pub struct HybridStatsSnapshot {
    pub promotions: u64,
    pub demotions: u64,
    pub evictions: u64,
    pub fast_bytes_used: u64,
    pub slow_bytes_used: u64,
    pub fast_objects: u64,
    pub slow_objects: u64,
}

/// Adapter around your in-process paper_cache library.
/// Uses PaperCache<u64, Box<[u8]>> internally and converts to/from Vec<u8> for the trait.

//this will be all_dram config..... need alternative for pmem//// 
//#[cfg(not(feature = "allocator_api",feature = "hybrid" ))]

#[cfg(not(any(feature = "allocator_api", feature = "hybrid", feature = "hybrid_lfu", feature = "hybrid_2q", feature = "hybrid_2q_fast_admission", feature = "hybrid_fifo", feature = "hybrid_lru_sized", feature = "hybrid_s3_fifo", feature = "hybrid_2q_ghost", feature = "hybrid_s3_fifo_ghost", feature = "hybrid_s3_fifo_ghost_lazy_demotion", feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission", feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission_midpoint", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_midpoint_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_split_slow_reprieve")))]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, Box<[u8]>>,
}

#[cfg(feature = "allocator_api")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, BufferPMEM>,
}

#[cfg(feature = "hybrid")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64,TieredBuffer>,
}

#[cfg(feature = "hybrid_lfu")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_2q")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_2q_fast_admission")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}
#[cfg(feature = "hybrid_2q_fast_admission")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Same FAST_TIER_GB convention and 0.1 k_in as `hybrid_2q`, kept
        // identical so the two are directly comparable -- the whole point of
        // this design is to isolate the effect of moving the one-access queue
        // to the fast tier, which only holds if nothing else differs.
        //
        // Worth knowing when sweeping, though: unlike `hybrid_2q`, k_in here
        // is a DRAM reservation carved out of FAST_TIER_GB rather than an
        // independent PMEM budget. At a 24 GB cache and FAST_TIER_GB=4, k_in
        // 0.1 reserves 2.4 GB of the 4 GB fast tier for objects with no
        // demonstrated reuse, leaving 1.6 GB for the main queue. Sweeping
        // k_in down is likely worthwhile here in a way it is not for
        // `hybrid_2q`.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let k_in: f64 = std::env::var("TWO_Q_K_IN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.1);

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
            k_in,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_fifo")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_lru_sized")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_s3_fifo")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_2q_ghost")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_s3_fifo_ghost")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_s3_fifo_ghost_lazy_demotion")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission_midpoint")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_midpoint_reprieve")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_reprieve")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_split_slow_reprieve")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}

#[cfg(feature = "hybrid_s3_fifo_lazy_demotion_reprieve")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, TieredBuffer>,
}


/*
#[cfg(feature = "allocator_api")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, Box<[u8]>>,
}
*/

#[cfg(not(any(feature = "allocator_api", feature = "hybrid", feature = "hybrid_lfu", feature = "hybrid_2q", feature = "hybrid_2q_fast_admission", feature = "hybrid_fifo", feature = "hybrid_lru_sized", feature = "hybrid_s3_fifo", feature = "hybrid_2q_ghost", feature = "hybrid_s3_fifo_ghost", feature = "hybrid_s3_fifo_ghost_lazy_demotion", feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission", feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission_midpoint", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_midpoint_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_split_slow_reprieve")))]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Policy selectable via PAPER_POLICY so this non-hybrid baseline can be
        // matched to whichever hybrid it is compared against. Defaults to Lfu,
        // the previous hardcoded value.
        let policy = match std::env::var("PAPER_POLICY").ok().as_deref() {
            Some("fifo") => PaperPolicy::Fifo,
            Some("lru") => PaperPolicy::Lru,
            _ => PaperPolicy::Lfu,
        };
        let cache = PaperCache::<u64, Box<[u8]>>::new(max_size, &[policy], policy)
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}



#[cfg(feature = "allocator_api")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let cache = PaperCache::<u64, BufferPMEM>::new(
            max_size,
            &[PaperPolicy::Lfu],
            PaperPolicy::Lfu,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Fast-tier size in GB, overridable via FAST_TIER_GB (defaults to 4,
        // the prior hardcoded value) so a fast-tier sweep doesn't need a
        // rebuild per size -- matches hybrid_lfu's pattern below. Parsed as
        // f64 (not u64) and converted to CacheTierSize::Mb so fractional
        // values like "2.5" work -- CacheTierSize::Gb only takes a whole
        // u64, which can't express 2.5 GB directly.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);
        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_lfu")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Fast-tier size in GB, overridable via FAST_TIER_GB (defaults to 4,
        // the prior hardcoded value) so a fast-tier sweep doesn't need a
        // rebuild per size. Parsed as f64 (not u64) and converted to
        // CacheTierSize::Mb, matching the "hybrid" (LRU) block's approach,
        // so fractional values like "13.5"/"17.5" work -- CacheTierSize::Gb
        // only takes a whole u64, which can't express a fractional GB.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);
        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_2q")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Parsed as f64 and converted to CacheTierSize::Mb so fractional
        // sweep values like "0.5"/"2.5" work. As u64 this silently fell back
        // to the 4 GB default for any fractional FAST_TIER_GB
        // (`"0.5".parse::<u64>()` fails, and the `.ok()` swallowed it), so a
        // sweep passing fractional GB measured 4 GB every time. Mb and Gb are
        // both decimal in `CacheTierSize` (10^6 / 10^9), so Mb(gb * 1000) is
        // exactly the old Gb(gb) for whole values -- existing whole-GB results
        // stay directly comparable.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
            0.1,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_fifo")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Parsed as f64 and converted to CacheTierSize::Mb so fractional
        // sweep values like "0.5"/"2.5" work. As u64 this silently fell back
        // to the 4 GB default for any fractional FAST_TIER_GB
        // (`"0.5".parse::<u64>()` fails, and the `.ok()` swallowed it), so a
        // sweep passing fractional GB measured 4 GB every time. Mb and Gb are
        // both decimal in `CacheTierSize` (10^6 / 10^9), so Mb(gb * 1000) is
        // exactly the old Gb(gb) for whole values -- existing whole-GB results
        // stay directly comparable.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_lru_sized")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // FAST_TIER_GB is the *total* fast-tier budget (matching every other
        // hybrid design's env-var sweep convention), split evenly between
        // the small and large segments. size_threshold is fixed at 16 KiB
        // (16384 bytes), close to these traces' ~16.1-16.5 KB average
        // object size, so both segments see genuine traffic rather than one
        // being empty.
        // Parsed as f64, not u64, so fractional sweep values like "0.5"/"2.5"
        // work. As u64 this silently fell back to the 4 GB default for any
        // fractional FAST_TIER_GB (`"0.5".parse::<u64>()` fails, and the
        // `.ok()` swallowed it) -- a fast-tier sweep passing fractional GB
        // was measuring 4 GB every time, not the size it asked for.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let half_mb = (fast_tier_gb * 1000.0 / 2.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(half_mb),
            CacheTierSize::Mb(half_mb),
            CacheTierSize::Bytes(16384),
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_s3_fifo")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Same FAST_TIER_GB env-var sweep convention as every other hybrid.
        // one_access_ratio fixed at 0.1, matching hybrid_2q's k_in convention
        // (the one other hybrid design with an equivalent extra ratio param).
        // Parsed as f64 and converted to CacheTierSize::Mb so fractional
        // sweep values like "0.5"/"2.5" work. As u64 this silently fell back
        // to the 4 GB default for any fractional FAST_TIER_GB
        // (`"0.5".parse::<u64>()` fails, and the `.ok()` swallowed it), so a
        // sweep passing fractional GB measured 4 GB every time. Mb and Gb are
        // both decimal in `CacheTierSize` (10^6 / 10^9), so Mb(gb * 1000) is
        // exactly the old Gb(gb) for whole values -- existing whole-GB results
        // stay directly comparable.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
            0.1,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_2q_ghost")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Same FAST_TIER_GB env-var sweep convention and 0.1 ratio as
        // hybrid_2q -- kept identical to the non-ghost variant so results
        // are directly comparable.
        // Parsed as f64 and converted to CacheTierSize::Mb so fractional
        // sweep values like "0.5"/"2.5" work. As u64 this silently fell back
        // to the 4 GB default for any fractional FAST_TIER_GB
        // (`"0.5".parse::<u64>()` fails, and the `.ok()` swallowed it), so a
        // sweep passing fractional GB measured 4 GB every time. Mb and Gb are
        // both decimal in `CacheTierSize` (10^6 / 10^9), so Mb(gb * 1000) is
        // exactly the old Gb(gb) for whole values -- existing whole-GB results
        // stay directly comparable.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
            0.1,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_s3_fifo_ghost")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Same FAST_TIER_GB env-var sweep convention and 0.1 ratio as
        // hybrid_s3_fifo -- kept identical to the non-ghost variant so
        // results are directly comparable.
        // Parsed as f64 and converted to CacheTierSize::Mb so fractional
        // sweep values like "0.5"/"2.5" work. As u64 this silently fell back
        // to the 4 GB default for any fractional FAST_TIER_GB
        // (`"0.5".parse::<u64>()` fails, and the `.ok()` swallowed it), so a
        // sweep passing fractional GB measured 4 GB every time. Mb and Gb are
        // both decimal in `CacheTierSize` (10^6 / 10^9), so Mb(gb * 1000) is
        // exactly the old Gb(gb) for whole values -- existing whole-GB results
        // stay directly comparable.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
            0.1,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_s3_fifo_ghost_lazy_demotion")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Same FAST_TIER_GB env-var sweep convention and 0.1 ratio as
        // hybrid_s3_fifo_ghost -- kept identical to the ghost variant (the
        // only new mechanic this design adds is a demotion-time
        // reference-bit gate, entirely internal to the stack) so results
        // are directly comparable.
        // Parsed as f64 and converted to CacheTierSize::Mb so fractional
        // sweep values like "0.5"/"2.5" work. As u64 this silently fell back
        // to the 4 GB default for any fractional FAST_TIER_GB
        // (`"0.5".parse::<u64>()` fails, and the `.ok()` swallowed it), so a
        // sweep passing fractional GB measured 4 GB every time. Mb and Gb are
        // both decimal in `CacheTierSize` (10^6 / 10^9), so Mb(gb * 1000) is
        // exactly the old Gb(gb) for whole values -- existing whole-GB results
        // stay directly comparable.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
            0.1,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Same FAST_TIER_GB env-var sweep convention and 0.1 ratio as
        // hybrid_s3_fifo_ghost_lazy_demotion. The 0.1 one_access_ratio here
        // means something new though: it's no longer an independent
        // slow-tier budget, it's a real reservation carved out of
        // FAST_TIER_GB itself (see
        // s3_fifo_ghost_lazy_demotion_fast_admission_hybrid_stack.rs's
        // module doc) -- e.g. at FAST_TIER_GB=6 and a 24GB cache, 10% of
        // 24GB (2.4GB) comes out of the 6GB fast budget, leaving ~3.6GB
        // effective room for the main queue's fast segment.
        // Parsed as f64 and converted to CacheTierSize::Mb so fractional
        // sweep values like "0.5"/"2.5" work. As u64 this silently fell back
        // to the 4 GB default for any fractional FAST_TIER_GB
        // (`"0.5".parse::<u64>()` fails, and the `.ok()` swallowed it), so a
        // sweep passing fractional GB measured 4 GB every time. Mb and Gb are
        // both decimal in `CacheTierSize` (10^6 / 10^9), so Mb(gb * 1000) is
        // exactly the old Gb(gb) for whole values -- existing whole-GB results
        // stay directly comparable.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
            0.1,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission_midpoint")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Same FAST_TIER_GB env-var sweep convention and 0.1 ratio as
        // hybrid_s3_fifo_ghost_lazy_demotion_fast_admission -- kept
        // identical to the predecessor variant so results are directly
        // comparable. The only new mechanic this design adds (a mid-slow-
        // segment reference-bit checkpoint promoting a re-accessed key
        // early instead of always waiting for the tail) is entirely
        // internal to the stack and doesn't add a new sizing knob.
        // Parsed as f64 and converted to CacheTierSize::Mb so fractional
        // sweep values like "0.5"/"2.5" work. As u64 this silently fell back
        // to the 4 GB default for any fractional FAST_TIER_GB
        // (`"0.5".parse::<u64>()` fails, and the `.ok()` swallowed it), so a
        // sweep passing fractional GB measured 4 GB every time. Mb and Gb are
        // both decimal in `CacheTierSize` (10^6 / 10^9), so Mb(gb * 1000) is
        // exactly the old Gb(gb) for whole values -- existing whole-GB results
        // stay directly comparable.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
            0.1,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_midpoint_reprieve")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Same FAST_TIER_GB env-var sweep convention and 0.1 ratio as
        // hybrid_s3_fifo_ghost_lazy_demotion_fast_admission_midpoint --
        // kept identical to the predecessor variant so results are
        // directly comparable. The two mechanics this design changes (no
        // ghost queue; a one-access key that ages out is spliced into the
        // slow tier instead of being evicted) are entirely internal to the
        // stack and don't add a new sizing knob.
        // Parsed as f64 and converted to CacheTierSize::Mb so fractional
        // sweep values like "0.5"/"2.5" work. As u64 this silently fell back
        // to the 4 GB default for any fractional FAST_TIER_GB
        // (`"0.5".parse::<u64>()` fails, and the `.ok()` swallowed it), so a
        // sweep passing fractional GB measured 4 GB every time. Mb and Gb are
        // both decimal in `CacheTierSize` (10^6 / 10^9), so Mb(gb * 1000) is
        // exactly the old Gb(gb) for whole values -- existing whole-GB results
        // stay directly comparable.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
            0.1,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_reprieve")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Same FAST_TIER_GB env-var sweep convention and 0.1 ratio as
        // every other variant in this lineage, so results stay directly
        // comparable. This variant only *removes* the mid-slow-tier
        // checkpoint; it adds no sizing knob.
        // Parsed as f64 and converted to CacheTierSize::Mb so fractional
        // sweep values like "0.5"/"2.5" work. As u64 this silently fell back
        // to the 4 GB default for any fractional FAST_TIER_GB
        // (`"0.5".parse::<u64>()` fails, and the `.ok()` swallowed it), so a
        // sweep passing fractional GB measured 4 GB every time. Mb and Gb are
        // both decimal in `CacheTierSize` (10^6 / 10^9), so Mb(gb * 1000) is
        // exactly the old Gb(gb) for whole values -- existing whole-GB results
        // stay directly comparable.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
            0.1,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_s3_fifo_lazy_demotion_reprieve")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Same FAST_TIER_GB env-var sweep convention and 0.1 ratio as
        // every other variant in this lineage, so results stay directly
        // comparable. This variant only *removes* the mid-slow-tier
        // checkpoint; it adds no sizing knob.
        // Parsed as f64 and converted to CacheTierSize::Mb so fractional
        // sweep values like "0.5"/"2.5" work. As u64 this silently fell back
        // to the 4 GB default for any fractional FAST_TIER_GB
        // (`"0.5".parse::<u64>()` fails, and the `.ok()` swallowed it), so a
        // sweep passing fractional GB measured 4 GB every time. Mb and Gb are
        // both decimal in `CacheTierSize` (10^6 / 10^9), so Mb(gb * 1000) is
        // exactly the old Gb(gb) for whole values -- existing whole-GB results
        // stay directly comparable.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
            0.1,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_split_slow_reprieve")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Same FAST_TIER_GB env-var sweep convention and 0.1 ratio as
        // every other variant in this lineage, so results stay directly
        // comparable. Splitting the slow tier into two segments is
        // internal to the stack (the split point is a fixed 50% of the
        // slow tier's bytes) and adds no new sizing knob here.
        // Parsed as f64 and converted to CacheTierSize::Mb so fractional
        // sweep values like "0.5"/"2.5" work. As u64 this silently fell back
        // to the 4 GB default for any fractional FAST_TIER_GB
        // (`"0.5".parse::<u64>()` fails, and the `.ok()` swallowed it), so a
        // sweep passing fractional GB measured 4 GB every time. Mb and Gb are
        // both decimal in `CacheTierSize` (10^6 / 10^9), so Mb(gb * 1000) is
        // exactly the old Gb(gb) for whole values -- existing whole-GB results
        // stay directly comparable.
        let fast_tier_gb: f64 = std::env::var("FAST_TIER_GB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4.0);

        let fast_tier_mb = (fast_tier_gb * 1000.0).round() as u64;

        let cache = PaperCache::<u64, TieredBuffer>::new(
            max_size,
            CacheTierSize::Mb(fast_tier_mb),
            0.1,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}



impl CacheBackend for PaperCacheBackend {
    fn ping(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // in-process cache: nothing to ping
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Box<dyn Error + Send + Sync>> {
        let key_u64 = key.parse::<u64>().map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        match self.inner.get(&key_u64) {
            Ok(boxed) => {
                //println!("Retrieved object length: {}", boxed.len());
                //Ok(Some(boxed.to_vec()))
                Ok(Some(boxed))
            }
            Err(err) => match err {
                PcError::KeyNotFound => Ok(None),
                other => Err(Box::new(other) as Box<dyn Error + Send + Sync>),
            },
        }
    }

    fn set(&self, key: String, value: Box<[u8]>, ttl: Option<u32>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let key_u64 = key.parse::<u64>().map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        //let boxed: Box<[u8]> = value.into_boxed_slice();

        #[cfg(feature = "value_dram")] {
            let boxed: Box<[u8], ValueDRAM> = Box::clone_from_ref_in(&value, ValueDRAM);
            self.inner.set(key_u64, &boxed, ttl).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)  
        }

        #[cfg(not(feature = "value_dram"))] {
            //let boxed: Box<[u8]> = value.into_boxed_slice();
            //self.inner.set(key_u64, &boxed, ttl).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
            self.inner.set(key_u64, &value, ttl).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
        }

        //println!("Set key: {}", key_u64);
        //self.inner.set(key_u64, &boxed, ttl).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
        //self.inner.set(key_u64, &value, ttl).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)


    

    }

    fn wipe(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.inner.wipe().map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    fn auth(&self, _token: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        // in-process cache has no auth
        Ok(())
    }

    fn cache_report(&self) -> Option<CacheReport> {
        let status = self.inner.status().ok()?;

        // One `#[cfg(any(..))]` over the hybrid features, not one arm per
        // design: `PaperCache::hybrid_stats()`/`fast_tier_size()` are single
        // methods on the shared generic `impl PaperCache<K, TieredBuffer, S>`
        // block regardless of which design is active (paper-cache resolves
        // that internally). Adding a new hybrid design to paper-cache only
        // requires adding its feature name to this list and to `Cargo.toml`.
        #[cfg(any(feature = "hybrid", feature = "hybrid_lfu", feature = "hybrid_2q", feature = "hybrid_2q_fast_admission", feature = "hybrid_fifo", feature = "hybrid_lru_sized", feature = "hybrid_s3_fifo", feature = "hybrid_2q_ghost", feature = "hybrid_s3_fifo_ghost", feature = "hybrid_s3_fifo_ghost_lazy_demotion", feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission", feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission_midpoint", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_midpoint_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_split_slow_reprieve"))]
        let (fast_tier_bytes, hybrid) = {
            let stats = self.inner.hybrid_stats();

            // `hybrid_lru_sized` is the one design where `fast_tier_size()`
            // is NOT the whole DRAM budget: it means the SMALL size segment
            // specifically, with the LARGE segment carried separately (see
            // paper-cache's `lru_sized_hybrid_cache` docs). This constructor
            // splits FAST_TIER_GB evenly across the two, so reporting only
            // `fast_tier_size()` here made `fast_bytes_used` (which *is* both
            // segments combined) look like ~195% of budget when it was
            // really ~97%. Summing the two is the honest total.
            #[cfg(feature = "hybrid_lru_sized")]
            let fast_tier_bytes = self.inner.fast_tier_size() + self.inner.large_fast_tier_size();

            #[cfg(not(feature = "hybrid_lru_sized"))]
            let fast_tier_bytes = self.inner.fast_tier_size();

            (
                Some(fast_tier_bytes),
                Some(HybridStatsSnapshot {
                    promotions: stats.promotions,
                    demotions: stats.demotions,
                    evictions: stats.evictions,
                    fast_bytes_used: stats.fast_bytes_used,
                    slow_bytes_used: stats.slow_bytes_used,
                    fast_objects: stats.fast_objects,
                    slow_objects: stats.slow_objects,
                }),
            )
        };

        #[cfg(not(any(feature = "hybrid", feature = "hybrid_lfu", feature = "hybrid_2q", feature = "hybrid_2q_fast_admission", feature = "hybrid_fifo", feature = "hybrid_lru_sized", feature = "hybrid_s3_fifo", feature = "hybrid_2q_ghost", feature = "hybrid_s3_fifo_ghost", feature = "hybrid_s3_fifo_ghost_lazy_demotion", feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission", feature = "hybrid_s3_fifo_ghost_lazy_demotion_fast_admission_midpoint", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_midpoint_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_reprieve", feature = "hybrid_s3_fifo_lazy_demotion_fast_admission_split_slow_reprieve")))]
        let (fast_tier_bytes, hybrid) = (None, None);

        Some(CacheReport {
            policy: status.policy().to_string(),
            max_size: status.max_size(),
            used_size: status.used_size(),
            num_objects: status.num_objects(),
            rss: status.rss(),
            hwm: status.hwm(),
            total_gets: status.total_gets(),
            total_sets: status.total_sets(),
            miss_ratio: status.miss_ratio(),
            fast_tier_bytes,
            hybrid,
        })
    }

}


