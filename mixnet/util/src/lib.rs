use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use tokio::net::TcpStream;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ConnectionPool {
    pool: Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<TcpStream>>>>>,
}

impl ConnectionPool {
    pub fn new(size: usize) -> Self {
        Self {
            pool: Arc::new(Mutex::new(HashMap::with_capacity(size))),
        }
    }

    pub async fn get_or_init(&self, addr: &SocketAddr) -> std::io::Result<Arc<Mutex<TcpStream>>> {
        let mut pool = self.pool.lock().await;
        match pool.get(addr).cloned() {
            Some(tcp) => Ok(tcp),
            None => {
                let tcp = Arc::new(Mutex::new(TcpStream::connect(addr).await?));
                pool.insert(*addr, tcp.clone());
                Ok(tcp)
            }
        }
    }
}
