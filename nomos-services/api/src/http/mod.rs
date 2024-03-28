pub type DynError = Box<dyn std::error::Error + Send + Sync + 'static>;
pub mod cl;
pub mod consensus;
pub mod libp2p;
pub mod mempool;
pub mod metrics;
pub mod storage;
