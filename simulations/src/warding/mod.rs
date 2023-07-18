// std
use std::sync::Arc;
// crates
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
// internal
use crate::runner::BoxedNode;

mod minmax;
mod stalled;
mod ttf;

pub struct SimulationState<S, T> {
    pub nodes: Arc<RwLock<Vec<BoxedNode<S, T>>>>,
}

impl<S, T> SimulationState<S, T> {
    #[inline]
    pub fn new(nodes: Vec<BoxedNode<S, T>>) -> Self {
        Self {
            nodes: Arc::new(RwLock::new(nodes)),
        }
    }
}

/// A ward is a computation over the `NetworkState`, it must return true if the state satisfies
/// the warding conditions. It is used to stop the consensus simulation if such condition is reached.
pub trait SimulationWard<S, T> {
    type SimulationState;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool;
}

/// Ward dispatcher
/// Enum to avoid Boxing (Box<dyn SimulationWard>) wards.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ward {
    MaxView(ttf::MaxViewWard),
    MinMaxView(minmax::MinMaxViewWard),
    StalledView(stalled::StalledViewWard),
}

impl Ward {
    pub fn simulation_ward_mut<S, T>(
        &mut self,
    ) -> &mut dyn SimulationWard<S, T, SimulationState = SimulationState<S, T>> {
        match self {
            Ward::MaxView(ward) => ward,
            Ward::MinMaxView(ward) => ward,
            Ward::StalledView(ward) => ward,
        }
    }
}

impl<S, T> SimulationWard<S, T> for Ward {
    type SimulationState = SimulationState<S, T>;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool {
        self.simulation_ward_mut().analyze(state)
    }
}
