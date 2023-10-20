pub type DynError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub mod backend;
pub(crate) mod cl;
pub(crate) mod da;
