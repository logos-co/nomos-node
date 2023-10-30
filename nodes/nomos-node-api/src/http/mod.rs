pub type DynError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub mod backend;
pub mod cl;
pub mod consensus;
pub mod da;
pub mod libp2p;
