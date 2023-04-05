use super::StateCache;
use crate::node::{Node, NodeId};
use indexmap::IndexMap;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FullTrackCachedState<S> {
    // use IndexMap here, because order is important
    states: IndexMap<NodeId, S>,
}

impl<S> FullTrackCachedState<S> {
    pub fn states(&self) -> &IndexMap<NodeId, S> {
        &self.states
    }
}

#[derive(Debug, Clone)]
pub struct FullTrackCache<S> {
    map: HashMap<usize, FullTrackCachedState<S>>,
}

impl<S: Clone> StateCache<S> for FullTrackCache<S> {
    fn new<N: Node<State = S>>(nodes: &[N]) -> Self {
        let mut map = HashMap::new();

        for n in nodes {
            let view = n.current_view();
            let id = n.id();
            map.entry(view)
                .and_modify(|states: &mut FullTrackCachedState<N::State>| {
                    states.states.insert(id, n.state().clone());
                })
                .or_insert_with(|| FullTrackCachedState {
                    states: [(id, n.state().clone())].into_iter().collect(),
                });
        }

        Self { map }
    }

    fn update_many<N: Node<State = S>>(&mut self, nodes: &[N]) {
        for n in nodes {
            let view = n.current_view();
            let id = n.id();
            self.map
                .entry(view)
                .and_modify(|states: &mut FullTrackCachedState<S>| {
                    states.states.insert(id, n.state().clone());
                })
                .or_insert_with(|| FullTrackCachedState {
                    states: [(id, n.state().clone())].into_iter().collect(),
                });
        }
    }

    fn update<N: Node<State = S>>(&mut self, node: &N) {
        let view = node.current_view();
        let id = node.id();
        self.map
            .entry(view)
            .and_modify(|states: &mut FullTrackCachedState<S>| {
                states.states.insert(id, node.state().clone());
            })
            .or_insert_with(|| FullTrackCachedState {
                states: [(id, node.state().clone())].into_iter().collect(),
            });
    }
}
