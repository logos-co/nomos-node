use crate::node::Node;
use crate::warding::{SimulationState, SimulationWard};
use consensus_engine::View;
use serde::{Deserialize, Serialize};

/// Time to finality ward. It monitors the amount of rounds of the simulations, triggers when surpassing
/// the set threshold.
#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
#[serde(transparent)]
pub struct MaxViewWard {
    max_view: View,
}

impl<N: Node> SimulationWard<N> for MaxViewWard {
    type SimulationState = SimulationState<N>;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool {
        state
            .nodes
            .read()
            .iter()
            .all(|n| n.current_view() >= self.max_view)
    }
}

#[cfg(test)]
mod test {
    use crate::warding::ttf::MaxViewWard;
    use crate::warding::{SimulationState, SimulationWard};
    use consensus_engine::View;
    use parking_lot::RwLock;
    use std::sync::Arc;

    #[test]
    fn rebase_threshold() {
        let mut ttf = MaxViewWard {
            max_view: View::new(10),
        };

        let node = 11;
        let state = SimulationState {
            nodes: Arc::new(RwLock::new(vec![node])),
        };
        assert!(ttf.analyze(&state));

        state.nodes.write().push(9);
        assert!(!ttf.analyze(&state));
    }
}
