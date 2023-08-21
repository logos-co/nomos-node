use std::net::SocketAddr;

use futures::Sink;
use mixnet_topology::MixnetTopology;
use serde::{Deserialize, Serialize};

use crate::receiver::Receiver;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetClientConfig {
    pub mode: MixnetClientMode,
    pub topology: MixnetTopology,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MixnetClientMode {
    Sender,
    SenderReceiver(SocketAddr),
}

impl MixnetClientMode {
    pub(crate) async fn run(
        &self,
        message_tx: impl Sink<Vec<u8>> + Clone + Unpin + Send + 'static,
    ) {
        match self {
            Self::Sender => (),
            Self::SenderReceiver(node_address) => {
                Receiver::run(*node_address, message_tx).await.unwrap()
            }
        }
    }
}
