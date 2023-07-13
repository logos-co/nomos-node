// std
use std::sync::Arc;
// crates
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
// internal
use crate::node::{
    carnot::{CarnotSettings, CarnotState},
    Node,
};

mod minmax;
mod stalled;
mod ttf;

pub struct SimulationState {
    pub nodes: Arc<
        RwLock<Vec<Box<dyn Node<Settings = CarnotSettings, State = CarnotState> + Send + Sync>>>,
    >,
}

impl SimulationState {
    #[inline]
    pub fn new(
        nodes: Vec<Box<dyn Node<Settings = CarnotSettings, State = CarnotState> + Send + Sync>>,
    ) -> Self {
        Self {
            nodes: Arc::new(RwLock::new(nodes)),
        }
    }
}

/// A ward is a computation over the `NetworkState`, it must return true if the state satisfies
/// the warding conditions. It is used to stop the consensus simulation if such condition is reached.
pub trait SimulationWard {
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
    pub fn simulation_ward_mut(
        &mut self,
    ) -> &mut dyn SimulationWard<SimulationState = SimulationState> {
        match self {
            Ward::MaxView(ward) => ward,
            Ward::MinMaxView(ward) => ward,
            Ward::StalledView(ward) => ward,
        }
    }
}

impl SimulationWard for Ward {
    type SimulationState = SimulationState;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool {
        self.simulation_ward_mut().analyze(state)
    }
}
