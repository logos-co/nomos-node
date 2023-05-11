use crate::node::{Node, NodeId};
use crate::runner::SimulationRunner;
use crate::warding::SimulationState;
use crossbeam::channel::bounded;
use crossbeam::select;
use rand::prelude::IteratorRandom;
use serde::Serialize;
use std::collections::BTreeSet;
use std::sync::Arc;

use super::SimulationRunnerHandle;

/// Simulate with sending the network state to any subscriber.
///
/// [Glauber dynamics simulation](https://en.wikipedia.org/wiki/Glauber_dynamics)
pub fn simulate<M, N: Node, R>(
    runner: SimulationRunner<M, N, R>,
    update_rate: usize,
    maximum_iterations: usize,
) -> anyhow::Result<SimulationRunnerHandle<R>>
where
    M: Send + Sync + Clone + 'static,
    N: Send + Sync + 'static,
    N::Settings: Clone + Send,
    N::State: Serialize,
    R: for<'a> TryFrom<&'a SimulationState<N>, Error = anyhow::Error> + Send + Sync + 'static,
{
    let simulation_state = SimulationState {
        nodes: Arc::clone(&runner.nodes),
    };

    let inner_runner = runner.inner.clone();
    let nodes = runner.nodes;
    let nodes_remaining: BTreeSet<NodeId> =
        (0..nodes.read().len())
            .map(From::from)
            .collect();
    let iterations: Vec<_> = (0..maximum_iterations).collect();
    let (stop_tx, stop_rx) = bounded(1);
    let p = runner.producer.clone();
    let p1 = runner.producer;
    let handle = std::thread::spawn(move || {
        let mut inner_runner: parking_lot::RwLockWriteGuard<super::SimulationRunnerInner<M>> =
            inner_runner.write();

        'main: for chunk in iterations.chunks(update_rate) {
            select! {
                recv(stop_rx) -> _ => break 'main,
                default => {
                    for _ in chunk {
                        if nodes_remaining.is_empty() {
                            break 'main;
                        }

                        let node_id = *nodes_remaining.iter().choose(&mut inner_runner.rng).expect(
                            "Some id to be selected as it should be impossible for the set to be empty here",
                        );

                        {
                            let mut shared_nodes = nodes.write();
                            let node: &mut N = shared_nodes
                                .get_mut(node_id.inner())
                                .expect("Node should be present");
                            node.step();
                        }

                        // check if any condition makes the simulation stop
                        if inner_runner.check_wards(&simulation_state) {
                            // we break the outer main loop, so we need to dump it before the breaking
                            p.send(R::try_from(
                                &simulation_state,
                            )?)?;
                            break 'main;
                        }
                    }
                    // update_rate iterations reached, so dump state
                    p.send(R::try_from(
                        &simulation_state,
                    )?)?;
                }
            }
        }
        Ok(())
    });
    Ok(SimulationRunnerHandle {
        producer: p1,
        stop_tx,
        handle,
    })
}
