use std::collections::HashMap;

use crate::node::Node;
use crate::warding::{SimulationState, SimulationWard};
use serde::Deserialize;

/// StalledView. Track stalled nodes (e.g incoming queue is empty, the node doesn't write to other queues)
#[derive(Debug, Deserialize, Clone)]
pub struct StalledViewWard<N: Eq + core::hash::Hash> {
    // the key is the node, the first element in the value is the node latest view, the second element
    // is the node how many times the node is not updated.
    checkpoints: HashMap<N, (usize, usize)>,
    // use to check if the node is stalled
    criterion: usize,
}

impl<N: Node + Clone + Eq + core::hash::Hash> SimulationWard<N> for StalledViewWard<N> {
    type SimulationState = SimulationState<N>;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool {
        let nodes = state
            .nodes
            .read()
            .expect("simulations: StalledViewWard panic when requiring a read lock");
        for node in nodes.iter() {
            let view = node.current_view();
            if let Some((last_view, count)) = self.checkpoints.get_mut(node) {
                if view == *last_view {
                    *count += 1;
                    if *count >= self.criterion {
                        return true;
                    }
                } else {
                    *last_view = view;
                    *count = 0;
                }
            } else {
                self.checkpoints.insert(node.clone(), (view, 0));
            }
        }

        false
    }
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, RwLock};
    use super::*;

    #[test]
    fn rebase_threshold() {
        let mut stalled = StalledViewWard {
            checkpoints: HashMap::new(),
            criterion: 2,
        };
        let state = SimulationState {
            nodes: Arc::new(RwLock::new(vec![10])),
        };

        // increase the criterion, 1
        assert!(!stalled.analyze(&state));
        // increase the criterion, 2
        assert!(!stalled.analyze(&state));
        // increase the criterion, 3
        assert!(stalled.analyze(&state));
    }
}
