mod api_backend;

use overwatch_rs::services::handle::ServiceHandle;
use overwatch_rs::Services;

use nomos_log::Logger;
use nomos_node::Wire;
use nomos_storage::{backends::sled::SledBackend, StorageService};

#[derive(Services)]
struct Explorer {
    log: ServiceHandle<Logger>,
    storage: ServiceHandle<StorageService<SledBackend<Wire>>>,
}

fn main() {
    println!("Hello, world!");
}
