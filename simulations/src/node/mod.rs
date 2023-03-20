pub mod carnot;

// std
use std::{fmt::Debug, time::Duration};
// crates
use rand::Rng;
// internal

pub type NodeId = usize;
pub type CommitteeId = usize;
pub type StepTime = Duration;

pub trait Node: Clone {
    type Role: Debug + Clone;
    type Settings;
    type Step;
    fn new<R: Rng>(rng: &mut R, id: NodeId, role: Self::Role, settings: Self::Settings) -> Self;
    fn id(&self) -> NodeId;
    fn run_step(&mut self, steps: Self::Step) -> StepTime;
}
