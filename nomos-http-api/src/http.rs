type DynError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub(crate) mod da;

pub mod backend;
