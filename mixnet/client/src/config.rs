use std::net::SocketAddr;

use futures::{stream, StreamExt};
use mixnet_topology::MixnetTopology;
use mixnet_util::ConnectionCache;
use serde::{Deserialize, Serialize};

use crate::{receiver::Receiver, MessageStream, MixnetClientError};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetClientConfig {
    pub mode: MixnetClientMode,
    pub topology: MixnetTopology,
    pub connection_cache_size: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MixnetClientMode {
    Sender,
    SenderReceiver(SocketAddr),
}

impl MixnetClientMode {
    pub(crate) async fn run(
        &self,
        cache: ConnectionCache,
    ) -> Result<MessageStream, MixnetClientError> {
        match self {
            Self::Sender => Ok(stream::empty().boxed()),
            Self::SenderReceiver(node_address) => {
                Ok(Receiver::new(*node_address, cache).run().await?.boxed())
            }
        }
    }
}
