mod nodes;
pub use nodes::NomosNode;

// std
use std::fmt::Debug;
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::Duration;

//crates
use fraction::Fraction;
use once_cell::sync::Lazy;
use rand::{Rng, SeedableRng};
use rand_xoshiro::Xoshiro256PlusPlus;

static RNG: Lazy<Mutex<Xoshiro256PlusPlus>> =
    Lazy::new(|| Mutex::new(Xoshiro256PlusPlus::seed_from_u64(42)));

pub fn get_available_port() -> u16 {
    let mut port: u16 = RNG.lock().unwrap().gen_range(8000..10000);
    while TcpListener::bind(("127.0.0.1", port)).is_err() {
        port += 1;
    }
    port
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
    Star {
        n_participants: usize,
        threshold: Fraction,
        timeout: Duration,
    },
}
