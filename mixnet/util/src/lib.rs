use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use crossbeam_skiplist::SkipMap;

use tokio::{
    net::TcpStream,
    sync::{
        mpsc::{error::SendError, UnboundedSender},
        Mutex,
    },
    task::JoinHandle,
};

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

    pub async fn get_or_init(
        &self,
        addr: &SocketAddr,
    ) -> mixnet_protocol::Result<Arc<Mutex<TcpStream>>> {
        let mut pool = self.pool.lock().await;
        match pool.get(addr).cloned() {
            Some(tcp) => Ok(tcp),
            None => {
                let tcp = Arc::new(Mutex::new(
                    TcpStream::connect(addr)
                        .await
                        .map_err(mixnet_protocol::ProtocolError::IO)?,
                ));
                pool.insert(*addr, tcp.clone());
                Ok(tcp)
            }
        }
    }
}

pub struct MessageHandle<M> {
    handle: Arc<JoinHandle<()>>,
    sender: UnboundedSender<M>,
}

impl<M> Clone for MessageHandle<M> {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
            sender: self.sender.clone(),
        }
    }
}

impl<M> MessageHandle<M> {
    pub fn new(sender: UnboundedSender<M>, handle: JoinHandle<()>) -> Self {
        Self {
            handle: Arc::new(handle),
            sender,
        }
    }

    pub fn send(&self, msg: M) -> Result<(), SendError<M>> {
        self.sender.send(msg)
    }
}

pub struct MessagePool<M> {
    pool: Arc<SkipMap<SocketAddr, MessageHandle<M>>>,
}

impl<M> Default for MessagePool<M> {
    fn default() -> Self {
        Self {
            pool: Arc::new(SkipMap::new()),
        }
    }
}

impl<M> Clone for MessagePool<M> {
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
        }
    }
}

impl<M> MessagePool<M>
where
    M: Send + 'static,
{
    pub fn new() -> Self {
        Self {
            pool: Arc::new(SkipMap::new()),
        }
    }

    pub fn get(&self, socket: &SocketAddr) -> Option<MessageHandle<M>> {
        self.pool.get(socket).and_then(|ent| {
            let val = ent.value();
            // check if the message handling thread for this socket exits
            // if it exits, then remove it from the pool
            if val.handle.is_finished() {
                ent.remove();
                None
            } else {
                Some(val.clone())
            }
        })
    }

    pub fn insert(&self, socket: SocketAddr, handle: MessageHandle<M>) {
        self.pool.insert(socket, handle);
    }
}
