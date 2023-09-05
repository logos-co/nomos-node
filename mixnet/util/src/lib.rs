use std::{net::SocketAddr, sync::Arc};

use crossbeam_skiplist::SkipMap;

// use parking_lot::Mutex;
use spin::RwLock;
use tokio::net::TcpStream;

#[derive(Clone)]
pub struct ConnectionPool {
    pool: Arc<SkipMap<SocketAddr, Arc<RwLock<TcpStream>>>>,
}

impl ConnectionPool {
    pub fn new(_size: usize) -> Self {
        Self {
            pool: Arc::new(SkipMap::new()),
        }
    }

    pub fn get(&self, addr: &SocketAddr) -> Option<Arc<RwLock<TcpStream>>> {
        self.pool.get(addr).map(|s| s.value().clone())
    }

    #[allow(clippy::await_holding_lock)]
    pub async fn get_or_init(&self, addr: &SocketAddr) -> std::io::Result<Arc<RwLock<TcpStream>>> {
        match self.pool.get(addr) {
            Some(ent) => Ok(ent.value().clone()),
            None => {
                let tcp = Arc::new(RwLock::new(TcpStream::connect(addr).await?));
                self.insert(*addr, tcp.clone());
                Ok(tcp)
            }
        }
    }

    pub fn insert(&self, addr: SocketAddr, stream: Arc<RwLock<TcpStream>>) {
        self.pool.insert(addr, stream);
    }
}
