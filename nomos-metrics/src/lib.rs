use prometheus_client::registry::Registry;
use std::sync::{Arc, Mutex};

pub use prometheus_client::{self, *};

// A wrapper for prometheus_client Registry.
// Lock is only used during services initialization.
pub type NomosRegistry = Arc<Mutex<Registry>>;

