use std::{error::Error, net::SocketAddr};

use futures::Stream;
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
    ) -> Result<
        Option<impl Stream<Item = Result<Vec<u8>, Box<dyn Error>>> + Send + 'static>,
        Box<dyn Error>,
    > {
        match self {
            Self::Sender => Ok(None),
            Self::SenderReceiver(node_address) => Receiver::run(*node_address).await.map(Some),
        }
    }
}
