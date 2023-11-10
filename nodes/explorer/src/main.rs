mod api_backend;

use nomos_api::ApiService;
use overwatch_rs::services::handle::ServiceHandle;
use overwatch_rs::Services;

use crate::api_backend::AxumBackend;
use nomos_log::Logger;
use nomos_node::{Tx, Wire};
use nomos_storage::{backends::sled::SledBackend, StorageService};

#[derive(Services)]
struct Explorer {
    log: ServiceHandle<Logger>,
    storage: ServiceHandle<StorageService<SledBackend<Wire>>>,
    api: ServiceHandle<ApiService<AxumBackend<Tx, Wire>>>,
}

fn main() {
    println!("Hello, world!");
}
