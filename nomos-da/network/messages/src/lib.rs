pub mod common;
pub mod dispersal;
pub mod http;
pub mod packing;
pub mod replication;
pub mod sampling;

type Result<T> = std::result::Result<T, std::io::Error>;
type SubnetworkId = u16; // Must match `nomos-da-network-core::SubnetworkId`
