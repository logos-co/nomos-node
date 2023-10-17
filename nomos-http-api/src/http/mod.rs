type DynError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub(crate) mod da;
pub(crate) mod cl;

pub mod backend;