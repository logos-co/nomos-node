use crate::node::Node;
use crate::warding::{SimulationState, SimulationWard};
use serde::Deserialize;

/// StalledView. Track stalled nodes (e.g incoming queue is empty, the node doesn't write to other queues)
#[derive(Debug, Deserialize, Clone)]
pub struct StalledViewWard {
    // the hash checksum
    #[serde(skip_deserializing)]
    consecutive_viewed_checkpoint: Option<u32>,
    // use to check if the node is stalled
    criterion: usize,
    threshold: usize,
}

impl StalledViewWard {
    fn update_state(&mut self, cks: u32) {
        match &mut self.consecutive_viewed_checkpoint {
            Some(cp) => {
                if cks == *cp {
                    self.criterion += 1;
                } else {
                    *cp = cks;
                    // reset the criterion
                    self.criterion = 0;
                }
            }
            None => {
                self.consecutive_viewed_checkpoint = Some(cks);
            }
        }
    }
}

impl<N: Node> SimulationWard<N> for StalledViewWard {
    type SimulationState = SimulationState<N>;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool {
        let nodes = state
            .nodes
            .read()
            .expect("simulations: StalledViewWard panic when requiring a read lock");
        self.update_state(checksum(nodes.as_slice()));
        self.criterion >= self.threshold
    }
}

#[inline]
fn checksum<N: Node>(nodes: &[N]) -> u32 {
    let mut hash = crc32fast::Hasher::new();
    for node in nodes.iter() {
        hash.update(&node.current_view().to_be_bytes());
        // TODO: hash messages in the node
    }

    hash.finalize()
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::{Arc, RwLock};

    #[test]
    fn rebase_threshold() {
        let mut stalled = StalledViewWard {
            consecutive_viewed_checkpoint: None,
            criterion: 0,
            threshold: 2,
        };
        let state = SimulationState {
            nodes: Arc::new(RwLock::new(vec![10])),
        };

        // increase the criterion, 1
        assert!(!stalled.analyze(&state));
        // increase the criterion, 2
        assert!(!stalled.analyze(&state));
        // increase the criterion, 3 > threshold 2, so true
        assert!(stalled.analyze(&state));

        // push a new one, so the criterion is reset to 0
        state.nodes.write().unwrap().push(20);
        assert!(!stalled.analyze(&state));

        // increase the criterion, 2
        assert!(!stalled.analyze(&state));
        // increase the criterion, 3 > threshold 2, so true
        assert!(stalled.analyze(&state));
    }
}
