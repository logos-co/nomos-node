// std
// crates
use serde::{Deserialize, Serialize};
// internal
use super::{Node, NodeId};

#[derive(Default, Serialize)]
pub struct CarnotState {}

#[derive(Clone, Deserialize)]
pub struct CarnotSettings {}

#[allow(dead_code)] // TODO: remove when handling settings
pub struct CarnotNode {
    id: NodeId,
    state: CarnotState,
    settings: CarnotSettings,
}

impl Node for CarnotNode {
    type Settings = CarnotSettings;
    type State = CarnotState;

    fn id(&self) -> NodeId {
        self.id
    }

    fn current_view(&self) -> usize {
        todo!()
    }

    fn state(&self) -> &CarnotState {
        &self.state
    }

    fn step(&mut self) {
        todo!()
    }
}
