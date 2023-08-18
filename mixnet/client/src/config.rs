use std::net::SocketAddr;

use mixnet_topology::MixnetTopology;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::receiver::Receiver;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetClientConfig {
    pub mode: MixnetClientMode,
    pub topology: MixnetTopology,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MixnetClientMode {
    Sender,
    SenderReceiver {
        listen_address: SocketAddr,
        channel_capacity: usize,
    },
}

impl MixnetClientMode {
    pub(crate) fn run(&self) -> Option<broadcast::Receiver<Vec<u8>>> {
        match self {
            Self::Sender => None,
            Self::SenderReceiver {
                listen_address,
                channel_capacity,
            } => {
                let (tx, rx) = broadcast::channel(*channel_capacity);
                let listen_address = *listen_address;

                tokio::spawn(async move { Receiver::run(listen_address, tx).await.unwrap() });

                Some(rx)
            }
        }
    }
}
