use super::StateCache;
use crate::node::{Node, NodeId};
use indexmap::IndexMap;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct LatestTrackCachedState<S> {
    // use IndexMap here, because order is important
    states: IndexMap<NodeId, S>,
}

impl<S> LatestTrackCachedState<S> {
    pub fn states(&self) -> &IndexMap<NodeId, S> {
        &self.states
    }
}

#[derive(Debug, Clone)]
pub struct LatestTrackCache<S> {
    map: HashMap<usize, LatestTrackCachedState<S>>,
}

impl<S: Clone> StateCache<S> for LatestTrackCache<S> {
    fn new<N: Node<State = S>>(nodes: &[N]) -> Self {
        let mut map = HashMap::new();

        for n in nodes {
            let view = n.current_view();
            let id = n.id();
            map.entry(view)
                .and_modify(|states: &mut LatestTrackCachedState<N::State>| {
                    states.states.insert(id, n.state().clone());
                })
                .or_insert_with(|| LatestTrackCachedState {
                    states: [(id, n.state().clone())].into_iter().collect(),
                });
        }

        Self { map }
    }

    fn update_many<N: Node<State = S>>(&mut self, nodes: &[N]) {
        // clear the old states for each node
        for node_states in self.map.values_mut() {
            for node in nodes.iter() {
                node_states.states.remove(&node.id());
            }
        }

        // insert the new states for each node
        for n in nodes {
            let view = n.current_view();
            let id = n.id();
            self.map
                .entry(view)
                .and_modify(|states: &mut LatestTrackCachedState<S>| {
                    states.states.insert(id, n.state().clone());
                })
                .or_insert_with(|| LatestTrackCachedState {
                    states: [(id, n.state().clone())].into_iter().collect(),
                });
        }
    }

    fn update<N: Node<State = S>>(&mut self, node: &N) {
        // clear the old states for each node
        for node_states in self.map.values_mut() {
            if node_states.states.remove(&node.id()).is_some() {
                break;
            }
        }

        // insert the node state
        let view = node.current_view();
        let id = node.id();
        self.map
            .entry(view)
            .and_modify(|states: &mut LatestTrackCachedState<S>| {
                states.states.insert(id, node.state().clone());
            })
            .or_insert_with(|| LatestTrackCachedState {
                states: [(id, node.state().clone())].into_iter().collect(),
            });
    }
}
