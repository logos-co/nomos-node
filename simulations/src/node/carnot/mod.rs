mod event_builder;
mod messages;

// std
// crates
use serde::{Deserialize, Serialize};
// internal
use super::{Node, NodeId};

#[derive(Default, Serialize)]
pub struct CarnotState {}

#[derive(Clone, Default, Deserialize)]
pub struct CarnotSettings {}

#[allow(dead_code)] // TODO: remove when handling settings
pub struct CarnotNode {
    id: NodeId,
    state: CarnotState,
    settings: CarnotSettings,
}

impl CarnotNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            state: Default::default(),
            settings: Default::default(),
        }
    }
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
