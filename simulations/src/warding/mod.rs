// std
use std::sync::{Arc, RwLock};
// crates
use serde::{Deserialize, Serialize};
// internal
use crate::{node::Node, output_processors::OutData};

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

impl<N> SimulationState<N>
where
    N: Node,
    N::State: Serialize,
{
    pub fn state(&self) -> Result<OutData, serde_json::Error> {
        serde_json::to_value(
            self.nodes
                .read()
                .expect("simulations: SimulationState panic when requiring a read lock")
                .iter()
                .map(N::state)
                .collect::<Vec<_>>(),
        )
        .map(OutData::new)
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
    Stalled(stalled::StalledViewWard),
}

impl Ward {
    pub fn simulation_ward_mut<N: Node>(
        &mut self,
    ) -> &mut dyn SimulationWard<N, SimulationState = SimulationState<N>> {
        match self {
            Ward::MaxView(ward) => ward,
            Ward::MinMaxView(ward) => ward,
            Ward::Stalled(ward) => ward,
        }
    }
}

impl<N: Node> SimulationWard<N> for Ward {
    type SimulationState = SimulationState<N>;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool {
        self.simulation_ward_mut().analyze(state)
    }
}
