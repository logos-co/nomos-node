use crate::node::Node;
use crate::warding::{SimulationState, SimulationWard};
use serde::Deserialize;

/// Time to finality ward. It monitors the amount of rounds of the simulations, triggers when surpassing
/// the set threshold.
#[derive(Debug, Deserialize, Copy, Clone)]
pub struct MinMaxViewWard {
    base_view: usize,
    max_gap: usize,
}

impl<N: Node> SimulationWard<N> for MinMaxViewWard {
    type SimulationState = SimulationState<N>;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool {
        state
            .nodes
            .read()
            .expect("simulations: MinMaxViewWard panic when requiring a read lock")
            .iter()
            .all(|n| n.current_view() >= self.max_gap + self.base_view)
    }
}

#[cfg(test)]
mod test {
    use crate::warding::minmax::MinMaxViewWard;
    use crate::warding::{SimulationState, SimulationWard};
    use std::sync::{Arc, RwLock};

    #[test]
    fn rebase_threshold() {
        let mut minmax = MinMaxViewWard {
            base_view: 10,
            max_gap: 5,
        };
        let state = SimulationState {
            nodes: Arc::new(RwLock::new(vec![10])),
        };
        // 10 - 10 = 0 < 5 so false
        assert!(!minmax.analyze(&state));
        state.nodes.write().unwrap()[0] = 15;
        // 15 - 10 = 5 = 5 so true
        assert!(minmax.analyze(&state));

        // push a new node with 9
        state.nodes.write().unwrap().push(9);
        // 15 - 9 = 6 > 5 so not all of nodes meet the condition, false
        assert!(!minmax.analyze(&state));

        // change the second node to 16
        state.nodes.write().unwrap()[1] = 16;
        // 16 - 15 = 1 < 5 so all of the nodes meet the analyze condition, true
        assert!(minmax.analyze(&state));
    }
}
