// std
// crates
use rand::Rng;
use serde::Deserialize;
// internal
use crate::node::{Node, NodeId};

use super::{NetworkState, SharedState};

#[derive(Debug, Default)]
pub struct DummyState {
    current_view: usize,
}

#[derive(Clone, Deserialize)]
pub struct DummySettings {}

pub struct DummyNode {
    id: NodeId,
    state: DummyState,
    _settings: DummySettings,
    _network_state: SharedState<NetworkState>,
}

impl Node for DummyNode {
    type Settings = DummySettings;
    type State = DummyState;

    fn new<R: Rng>(
        _rng: &mut R,
        id: NodeId,
        _settings: Self::Settings,
        _network_state: SharedState<NetworkState>,
    ) -> Self {
        Self {
            id,
            state: Default::default(),
            _settings,
            _network_state,
        }
    }

    fn id(&self) -> NodeId {
        self.id
    }

    fn current_view(&self) -> usize {
        self.state.current_view
    }

    fn state(&self) -> &DummyState {
        &self.state
    }

    fn step(&mut self) {
        self.state.current_view += 1;
    }
}
