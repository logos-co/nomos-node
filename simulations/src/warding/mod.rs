// std
use std::sync::{Arc, RwLock};
// crates
use serde::Deserialize;
// internal
use crate::node::Node;

mod minmax;
mod ttf;

pub struct SimulationState<N> {
    pub nodes: Arc<RwLock<Vec<N>>>,
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
pub enum Ward {
    #[serde(rename = "time_to_finality")]
    MaxView(ttf::MaxViewWard),
    #[serde(rename = "min_max_view")]
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
