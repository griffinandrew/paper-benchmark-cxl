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

/// Adapter around your in-process paper_cache library.
/// Uses PaperCache<u64, Box<[u8]>> internally and converts to/from Vec<u8> for the trait.

//this will be all_dram config..... need alternative for pmem//// 
#[cfg(not(feature = "allocator_api"))]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, Box<[u8]>>,
}

#[cfg(feature = "allocator_api")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, BufferPMEM>,
}



/*
#[cfg(feature = "allocator_api")]
pub struct PaperCacheBackend {
    inner: PaperCache<u64, Box<[u8]>>,
}
*/

#[cfg(not(feature = "allocator_api"))]
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



impl CacheBackend for PaperCacheBackend {
    fn ping(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // in-process cache: nothing to ping
        Ok(())
    }

    fn get(&mut self, key: &str) -> Result<Option<Vec<u8>>, Box<dyn Error + Send + Sync>> {
        let key_u64 = key.parse::<u64>().map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?;

        match self.inner.get(&key_u64) {
            Ok(boxed) => {
                //println!("Retrieved object length: {}", boxed.len());
                Ok(Some(boxed.to_vec()))
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
        //println!("Set key: {}", key_u64);
        self.inner.set(key_u64, &boxed, ttl).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    fn wipe(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.inner.wipe().map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)
    }

    fn auth(&mut self, _token: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        // in-process cache has no auth
        Ok(())
    }
}
