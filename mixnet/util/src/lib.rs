use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use parking_lot::Mutex;
use tokio::net::TcpStream;

#[derive(Clone)]
#[repr(transparent)]
pub struct ConnectionCache {
    cache: Arc<Mutex<HashMap<SocketAddr, Arc<tokio::sync::Mutex<TcpStream>>>>>,
}

impl ConnectionCache {
    pub fn new(size: usize) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::with_capacity(size))),
        }
    }

    pub fn get(&self, addr: &SocketAddr) -> Option<Arc<tokio::sync::Mutex<TcpStream>>> {
        self.cache.lock().get(addr).cloned()
    }

    pub fn insert(&self, addr: SocketAddr, stream: Arc<tokio::sync::Mutex<TcpStream>>) {
        self.cache.lock().insert(addr, stream);
    }
}
