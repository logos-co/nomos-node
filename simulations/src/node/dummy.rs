// std
// crates
use rand::Rng;
use serde::Deserialize;
// internal
use crate::node::{Node, NodeId};

#[derive(Default)]
pub struct DummyState {}

#[derive(Clone, Deserialize)]
pub struct DummySettings {}

#[allow(dead_code)] // TODO: remove when handling settings
pub struct DummyNode {
    id: NodeId,
    state: DummyState,
    settings: DummySettings,
}

impl Node for DummyNode {
    type Settings = DummySettings;
    type State = DummyState;

    fn new<R: Rng>(_rng: &mut R, id: NodeId, settings: Self::Settings) -> Self {
        Self {
            id,
            state: Default::default(),
            settings,
        }
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn current_view(&self) -> usize {
        todo!()
    }

    fn state(&self) -> &DummyState {
        &self.state
    }

    fn step(&mut self) {
        todo!()
    }
}
