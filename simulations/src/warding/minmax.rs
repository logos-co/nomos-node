use crate::warding::{SimulationState, SimulationWard};
use carnot_engine::View;
use serde::{Deserialize, Serialize};

/// MinMaxView. It monitors the gap between a min view and max view, triggers when surpassing
/// the max view - min view is larger than a gap.
#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
#[serde(transparent)]
pub struct MinMaxViewWard {
    max_gap: View,
}

impl<S, T> SimulationWard<S, T> for MinMaxViewWard {
    type SimulationState = SimulationState<S, T>;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool {
        let mut min = View::new(i64::MAX);
        let mut max = View::new(0);
        let nodes = state.nodes.read();
        for node in nodes.iter() {
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
    use consensus_engine::View;
    use parking_lot::RwLock;
    use std::sync::Arc;

    #[test]
    fn rebase_threshold() {
        let mut minmax = MinMaxViewWard {
            max_gap: View::new(5),
        };
        let state = SimulationState {
            nodes: Arc::new(RwLock::new(vec![Box::new(10)])),
        };
        // we only have one node, so always false
        assert!(!minmax.analyze(&state));

        // push a new node with 10
        state.nodes.write().push(Box::new(20));
        // we now have two nodes and the max - min is 10 > max_gap 5, so true
        assert!(minmax.analyze(&state));
    }
}
