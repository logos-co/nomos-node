use serde::{Serialize, Deserialize};

use super::{NodeId, Node};



#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize)]
pub struct DummyStreamingState {
  pub current_view: usize,
}

/// This node implementation only used for testing different streaming implementation purposes.
pub struct DummyStreamingNode<S> {
  id: NodeId,
  state: DummyStreamingState,
  settings: S,
}

impl<S> DummyStreamingNode<S> {
  pub fn new(id: NodeId, settgins: S) -> Self {
    Self {
      id,
      state: DummyStreamingState::default(),
      settings: settgins,
    }
  }
}

impl<S> Node for DummyStreamingNode<S> {
    type Settings = S;

    type State = DummyStreamingState;

    fn id(&self) -> NodeId {
        self.id
    }

    fn current_view(&self) -> usize {
        self.state.current_view
    }

    fn state(&self) -> &Self::State {
        &self.state
    }

    fn step(&mut self) {
        self.state.current_view += 1;
    }
}