use crate::node::Node;
use crate::warding::{SimulationState, SimulationWard};
use serde::Deserialize;

/// Time to finality ward. It monitors the amount of rounds of the simulations, triggers when surpassing
/// the set threshold.
#[derive(Debug, Deserialize, Copy, Clone)]
pub struct MaxViewWard {
    max_view: usize,
}

impl<N: Node> SimulationWard<N> for MaxViewWard {
    type SimulationState = SimulationState<N>;
    fn analyze(&mut self, _state: &Self::SimulationState) -> bool {
        self.max_view > 100 // TODO: implement using simulation state
    }
}

#[cfg(test)]
mod test {
    use crate::node::NetworkState;
    use crate::warding::ttf::MaxViewWard;
    use crate::warding::{SimulationState, SimulationWard};
    use std::sync::{Arc, RwLock};

    #[test]
    fn rebase_threshold() {
        let network_state = NetworkState::new(RwLock::new(vec![]));
        let mut ttf = MaxViewWard { ttf_threshold: 10 };
        let mut cond = false;
        let mut state = SimulationState {
            network_state,
            nodes: Arc::new(Default::default()),
            iteration: 0,
            round: 0,
        };
        for _ in 0..11 {
            state.round += 1;
            cond = ttf.analyze(&state);
        }
        assert!(cond);
    }
}
