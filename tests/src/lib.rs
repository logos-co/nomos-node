mod nodes;
pub use nodes::NomosNode;

// std
use std::fmt::Debug;
use std::net::TcpListener;
use std::time::Duration;

//crates
use fraction::Fraction;
use rand::{thread_rng, Rng};

pub fn get_available_port() -> u16 {
    let mut port: u16 = thread_rng().gen_range(8000..10000);
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
