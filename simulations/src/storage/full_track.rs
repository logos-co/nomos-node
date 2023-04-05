use super::StateCache;
use crate::node::{Node, NodeId};
use indexmap::IndexMap;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FullTrackCachedState<S> {
    // use IndexMap here, because order is important
    states: IndexMap<NodeId, S>,
}

// impl<S> FullTrackCachedState<S> {
//   pub fn states(&self) -> &[(NodeId, S)] {
//     &self.states
//   }
// }

#[derive(Debug, Clone)]
pub struct FullTrackCache<S> {
    map: HashMap<usize, FullTrackCachedState<S>>,
}

impl<'a, N: 'a, I: 'a> From<I> for FullTrackCache<N::State>
where
    N: Node,
    N::State: Clone,
    I: Iterator<Item = &'a N>,
{
    fn from(iter: I) -> Self {
        let mut map = HashMap::new();

        for n in iter {
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

impl<S> FullTrackCache<S> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn get(&self, view: usize) -> Option<&FullTrackCachedState<S>> {
        self.map.get(&view)
    }

    pub fn insert(&mut self, view: usize, state: FullTrackCachedState<S>) {
        self.map.insert(view, state);
    }

    pub fn update_many<N: Node>(&mut self, nodes: &[N]) {}
}
