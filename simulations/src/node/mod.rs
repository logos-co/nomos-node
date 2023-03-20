pub mod carnot;

// std
use std::time::Duration;
// crates
use rand::Rng;
// internal

pub type NodeId = usize;
pub type CommitteeId = usize;
pub type StepTime = Duration;

pub trait Node {
    type Settings: serde::Serialize + serde::de::DeserializeOwned;
    type Step: Step;
    fn new<R: Rng>(rng: &mut R, id: NodeId, settings: Self::Settings) -> Self;
    fn id(&self) -> NodeId;
    fn run_step(&mut self, steps: Self::Step) -> StepTime;
}

pub trait Step: serde::Serialize + serde::de::DeserializeOwned + core::str::FromStr {}
