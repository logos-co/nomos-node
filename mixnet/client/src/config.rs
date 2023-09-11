use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use futures::{stream, StreamExt};
use mixnet_topology::MixnetTopology;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{receiver::Receiver, MessageStream, MixnetClientError};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetClientConfig {
    pub mode: MixnetClientMode,
    pub topology: MixnetTopology,
    pub connection_pool_size: usize,
    pub max_retries: usize,
    pub retry_delay: std::time::Duration,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MixnetClientMode {
    Sender,
    SenderReceiver(SocketAddr),
}

impl MixnetClientMode {
    pub(crate) async fn run(&self) -> Result<MessageStream, MixnetClientError> {
        let ack_cache = Arc::new(Mutex::new(HashMap::new()));
        match self {
            // TODO: do not forget add ack_cache to the sender
            Self::Sender => Ok(stream::empty().boxed()),
            Self::SenderReceiver(node_address) => {
                Ok(Receiver::new(*node_address, ack_cache).run().await?.boxed())
            }
        }
    }
}
