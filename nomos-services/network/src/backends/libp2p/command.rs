use nomos_libp2p::{libp2p::kad, Multiaddr, PeerId};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

#[derive(Debug)]
#[non_exhaustive]
pub enum Command {
    Connect(Dial),
    Broadcast {
        topic: Topic,
        message: Box<[u8]>,
    },
    Subscribe(Topic),
    Unsubscribe(Topic),
    Info {
        reply: oneshot::Sender<Libp2pInfo>,
    },
    #[doc(hidden)]
    RetryBroadcast {
        topic: Topic,
        message: Box<[u8]>,
        retry_count: usize,
    },

    // Kademlia commands
    AddPeer {
        peer_id: PeerId,
        addr: Multiaddr,
        reply: oneshot::Sender<Result<(), String>>,
    },
    FindPeer {
        peer_id: PeerId,
        reply: oneshot::Sender<Result<Vec<Multiaddr>, String>>,
    },
    PutValue {
        key: Vec<u8>,
        value: Vec<u8>,
        reply: oneshot::Sender<Result<kad::RecordKey, String>>,
    },
    GetValue {
        key: Vec<u8>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    Bootstrap {
        reply: oneshot::Sender<Result<(), String>>,
    },
}

#[derive(Debug)]
pub struct Dial {
    pub addr: Multiaddr,
    pub retry_count: usize,
    pub result_sender: oneshot::Sender<Result<PeerId, nomos_libp2p::DialError>>,
}

pub type Topic = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Libp2pInfo {
    pub listen_addresses: Vec<Multiaddr>,
    pub n_peers: usize,
    pub n_connections: u32,
    pub n_pending_connections: u32,
}
