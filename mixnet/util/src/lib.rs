use std::{net::SocketAddr, num::NonZeroUsize, sync::Arc};

use lru::LruCache;
use parking_lot::Mutex;
use tokio::net::TcpStream;

/// The maximum number of open files allowed for the current process.
pub static MAX_OPEN_FILES_LIMIT: once_cell::sync::Lazy<Option<usize>> =
    once_cell::sync::Lazy::new(|| {
        use rustix::process::{getrlimit, Resource};
        // minus 1 because we need to leave one file descriptor for the listener
        getrlimit(Resource::Nofile).maximum.map(|n| n as usize)
    });

#[derive(Clone)]
#[repr(transparent)]
pub struct ConnectionCache {
    lru: Arc<Mutex<LruCache<SocketAddr, Arc<tokio::sync::Mutex<TcpStream>>>>>,
}

impl ConnectionCache {
    pub fn new(size: usize) -> Self {
        Self {
            lru: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(size.max(1)).unwrap(),
            ))),
        }
    }

    pub fn get(&self, addr: &SocketAddr) -> Option<Arc<tokio::sync::Mutex<TcpStream>>> {
        self.lru.lock().get(addr).cloned()
    }

    pub fn insert(&self, addr: SocketAddr, stream: Arc<tokio::sync::Mutex<TcpStream>>) {
        self.lru.lock().put(addr, stream);
    }
}
