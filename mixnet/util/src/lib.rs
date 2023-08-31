use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use parking_lot::Mutex;
use tokio::net::TcpStream;

#[derive(Clone)]
#[repr(transparent)]
pub struct ConnectionPool {
    pool: Arc<Mutex<HashMap<SocketAddr, Arc<tokio::sync::Mutex<TcpStream>>>>>,
}

impl ConnectionPool {
    pub fn new(size: usize) -> Self {
        Self {
            pool: Arc::new(Mutex::new(HashMap::with_capacity(size))),
        }
    }

    pub fn get(&self, addr: &SocketAddr) -> Option<Arc<tokio::sync::Mutex<TcpStream>>> {
        self.pool.lock().get(addr).cloned()
    }

    #[allow(clippy::await_holding_lock)]
    pub async fn get_or_init(
        &self,
        addr: &SocketAddr,
    ) -> std::io::Result<Arc<tokio::sync::Mutex<TcpStream>>> {
        let mut pool = self.pool.lock();
        match pool.get(addr) {
            Some(tcp) => Ok(tcp.clone()),
            None => {
                let tcp = Arc::new(tokio::sync::Mutex::new(TcpStream::connect(addr).await?));
                pool.insert(*addr, tcp.clone());
                Ok(tcp)
            }
        }
    }

    pub fn insert(&self, addr: SocketAddr, stream: Arc<tokio::sync::Mutex<TcpStream>>) {
        self.pool.lock().insert(addr, stream);
    }
}
