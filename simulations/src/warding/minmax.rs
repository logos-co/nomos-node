use crate::node::Node;
use crate::warding::{SimulationState, SimulationWard};
use serde::Deserialize;

/// MinMaxView. It monitors the gap between a min view and max view, triggers when surpassing
/// the max view - min view is larger than a gap.
#[derive(Debug, Deserialize, Copy, Clone)]
pub struct MinMaxViewWard {
    max_gap: usize,
}

impl<N: Node> SimulationWard<N> for MinMaxViewWard {
    type SimulationState = SimulationState<N>;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool {
        let mut min = 0;
        let mut max = 0;
        for node in state
            .nodes
            .read()
            .expect("simulations: MinMaxViewWard panic when requiring a read lock")
            .iter()
        {
            let view = node.current_view();
            min = min.min(view);
            max = max.max(view);
        }
        max - min >= self.max_gap
    }
}

#[cfg(test)]
mod test {
    use crate::warding::minmax::MinMaxViewWard;
    use crate::warding::{SimulationState, SimulationWard};
    use std::sync::{Arc, RwLock};

    #[test]
    fn rebase_threshold() {
        let mut minmax = MinMaxViewWard { max_gap: 5 };
        let state = SimulationState {
            nodes: Arc::new(RwLock::new(vec![10])),
        };
        // we only have one node, so always false
        assert!(!minmax.analyze(&state));

        // push a new node with 10
        state.nodes.write().unwrap().push(20);
        // we now have two nodes and the max - min is 10 > max_gap 5, so true
        assert!(minmax.analyze(&state));
    }
}
