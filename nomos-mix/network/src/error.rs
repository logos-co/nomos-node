use std::io;

use libp2p::{gossipsub, swarm::ConnectionId, PeerId};

#[derive(Debug)]
pub enum Error {
    /// There were no peers to send a message to.
    NoPeers,
    /// Gossipsub publish error
    GossipsubPublishError(gossipsub::PublishError),
    /// Gossipsub subscription error
    GossipsubSubscriptionError(gossipsub::SubscriptionError),
    /// IO error from peer
    PeerIOError {
        error: io::Error,
        peer_id: PeerId,
        connection_id: ConnectionId,
    },
}
