// std
use std::sync::{Arc, RwLock};
// crates
use serde::Deserialize;
// internal
use crate::node::Node;

mod minmax;
mod stalled;
mod ttf;

pub struct SimulationState<N> {
    pub nodes: Arc<RwLock<Vec<N>>>,
}

impl<N> SimulationState<N> {
    #[inline]
    pub fn new(nodes: Vec<N>) -> Self {
        Self {
            nodes: Arc::new(RwLock::new(nodes)),
        }
    }
}

/// A ward is a computation over the `NetworkState`, it must return true if the state satisfies
/// the warding conditions. It is used to stop the consensus simulation if such condition is reached.
pub trait SimulationWard<N> {
    type SimulationState;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool;
}

/// Ward dispatcher
/// Enum to avoid Boxing (Box<dyn SimulationWard>) wards.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ward {
    MaxView(ttf::MaxViewWard),
    MinMaxView(minmax::MinMaxViewWard),
}

impl Ward {
    pub fn simulation_ward_mut<N: Node>(
        &mut self,
    ) -> &mut dyn SimulationWard<N, SimulationState = SimulationState<N>> {
        match self {
            Ward::MaxView(ward) => ward,
            Ward::MinMaxView(ward) => ward,
        }
    }
}

impl<N: Node> SimulationWard<N> for Ward {
    type SimulationState = SimulationState<N>;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool {
        self.simulation_ward_mut().analyze(state)
    }
}
