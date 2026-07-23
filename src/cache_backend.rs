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

#[cfg(feature = "hybrid")]
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
}

/// Adapter around your in-process paper_cache library.
/// Uses PaperCache<u64, Box<[u8]>> internally and converts to/from Vec<u8> for the trait.

//this will be all_dram config..... need alternative for pmem//// 
//#[cfg(not(feature = "allocator_api",feature = "hybrid" ))]

#[cfg(not(any(feature = "allocator_api", feature = "hybrid")))]
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


/*
#[cfg(feature = "allocator_api")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, Box<[u8]>>,
}
*/

#[cfg(not(any(feature = "allocator_api", feature = "hybrid")))]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // default to a single LRU policy; change if you want CLI policy selection
        let cache = PaperCache::<u64, Box<[u8]>>::new(max_size, &[PaperPolicy::Lru], PaperPolicy::Lru)
            .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}



#[cfg(feature = "allocator_api")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // default to a single LRU policy; change if you want CLI policy selection
        let cache = PaperCache::<u64, BufferPMEM>::new(
            max_size,
            &[PaperPolicy::Lru],
            PaperPolicy::Lru,
        )
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;
        Ok(PaperCacheBackend { inner: cache })
    }
}

#[cfg(feature = "hybrid")]
impl PaperCacheBackend {
    pub fn new(max_size: u64) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // default to a single LRU policy; change if you want CLI policy selection
        let cache = PaperCache::<u64, TieredBuffer>::new(  
            max_size,
            //CacheTierSize::Mb(8192),
            CacheTierSize::Gb(4),
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

}


