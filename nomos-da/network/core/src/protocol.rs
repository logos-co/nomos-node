use libp2p::StreamProtocol;

pub const REPLICATION_PROTOCOL: StreamProtocol = StreamProtocol::new("/nomos/da/0.1.0/replication");
pub const DISPERSAL_PROTOCOL: StreamProtocol = StreamProtocol::new("/nomos/da/0.1.0/dispersal");
