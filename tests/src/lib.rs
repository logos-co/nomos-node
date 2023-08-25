mod nodes;
pub use nodes::MixNode;
pub use nodes::NomosNode;

// std
use std::fmt::Debug;
use std::time::Duration;

//crates
use fraction::Fraction;

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
