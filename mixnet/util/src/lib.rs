use std::{net::SocketAddr, sync::Arc, cell::UnsafeCell};

use crossbeam_skiplist::SkipMap;

// use parking_lot::Mutex;
// use spin::RwLock;
use tokio::net::TcpStream;

pub struct StreamCell {
    stream: UnsafeCell<TcpStream>,
}

impl StreamCell {
    pub fn get(&self) -> &TcpStream {
        unsafe { &*self.stream.get() }
    }

    #[allow(clippy::mut_from_ref)]
    pub fn get_mut(&self) -> &mut TcpStream {
        unsafe { &mut *self.stream.get() }
    }
}

impl From<UnsafeCell<TcpStream>> for StreamCell {
    fn from(stream: UnsafeCell<TcpStream>) -> Self {
        Self { stream }
    }
}

impl From<TcpStream> for StreamCell {
    fn from(stream: TcpStream) -> Self {
        Self {
            stream: UnsafeCell::new(stream),
        }
    }
}

unsafe impl Send for StreamCell {}
unsafe impl Sync for StreamCell {}

#[derive(Clone)]
pub struct ConnectionPool {
    pool: Arc<SkipMap<SocketAddr, Arc<StreamCell>>>,
}

impl ConnectionPool {
    pub fn new(_size: usize) -> Self {
        Self {
            pool: Arc::new(SkipMap::new()),
        }
    }

    pub fn get(&self, addr: &SocketAddr) -> Option<Arc<StreamCell>> {
        self.pool.get(addr).map(|s| s.value().clone())
    }

    #[allow(clippy::await_holding_lock)]
    pub async fn get_or_init(&self, addr: &SocketAddr) -> std::io::Result<Arc<StreamCell>> {
        match self.pool.get(addr) {
            Some(ent) => Ok(ent.value().clone()),
            None => {
                let tcp = Arc::new(StreamCell::from(TcpStream::connect(addr).await?));
                self.insert(*addr, tcp.clone());
                Ok(tcp)
            }
        }
    }

    pub fn insert(&self, addr: SocketAddr, stream: Arc<StreamCell>) {
        self.pool.insert(addr, stream);
    }
}
