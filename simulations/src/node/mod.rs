pub mod carnot;

// std
use std::time::Duration;
// crates
use rand::Rng;

use self::carnot::CarnotStep;
// internal

pub type NodeId = usize;
pub type CommitteeId = usize;
pub type StepTime = Duration;

pub trait Node: Clone {
    type Settings;
    fn new<R: Rng>(rng: &mut R, id: NodeId, settings: Self::Settings) -> Self;
    fn id(&self) -> NodeId;
    fn run_steps(&mut self, steps: &[CarnotStep]) -> StepTime;
}
