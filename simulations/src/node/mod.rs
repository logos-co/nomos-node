// std
use std::time::Duration;
// crates
// internal

pub type NodeId = usize;
pub type StepTime = Duration;

pub trait Node {
    type Settings;
    type Step;
    fn new(settings: Self::Settings) -> Self;
    fn id(&self) -> NodeId;
    fn run_step(&mut self, steps: Self::Step) -> StepTime;
}
