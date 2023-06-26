mod nodes;

pub use nodes::NomosNode;
use once_cell::sync::Lazy;
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
use std::fmt::Debug;
use std::net::TcpListener;
use std::sync::Mutex;

static RNG: Lazy<Mutex<Xoshiro256PlusPlus>> =
    Lazy::new(|| Mutex::new(Xoshiro256PlusPlus::seed_from_u64(42)));

static NET_PORT: Mutex<u16> = Mutex::new(8000);

pub fn get_available_port() -> u16 {
    let mut port = NET_PORT.lock().unwrap();
    *port += 1;
    while TcpListener::bind(("127.0.0.1", *port)).is_err() {
        *port += 1;
    }
    *port
}

#[async_trait::async_trait]
pub trait Node: Sized {
    type ConsensusInfo: Debug + Clone + PartialEq;
    async fn spawn_nodes(config: SpawnConfig) -> Vec<Self>;
    async fn consensus_info(&self) -> Self::ConsensusInfo;
    fn stop(&mut self);
}

#[derive(Clone, Copy)]
pub enum SpawnConfig {
    Star { n_participants: usize },
}
