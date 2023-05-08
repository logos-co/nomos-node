use crate::node::{Node, NodeId};
use crate::overlay::Overlay;
use crate::runner::{SimulationRunner, SimulationRunnerHandle};
use crate::streaming::{Producer, Subscriber};
use crate::warding::SimulationState;
use crossbeam::channel::bounded;
use crossbeam::select;
use rand::prelude::IteratorRandom;
use serde::Serialize;
use std::collections::BTreeSet;
use std::sync::Arc;

/// Simulate with sending the network state to any subscriber.
///
/// [Glauber dynamics simulation](https://en.wikipedia.org/wiki/Glauber_dynamics)
pub fn simulate<M, N: Node, O: Overlay, P: Producer>(
    runner: SimulationRunner<M, N, O, P>,
    update_rate: usize,
    maximum_iterations: usize,
) -> anyhow::Result<SimulationRunnerHandle>
where
    M: Send + Sync + Clone + 'static,
    N: Send + Sync + 'static,
    N::Settings: Clone + Send,
    N::State: Serialize,
    O::Settings: Clone + Send,
    P::Subscriber: Send + Sync + 'static,
    <P::Subscriber as Subscriber>::Record:
        for<'a> TryFrom<&'a SimulationState<N>, Error = anyhow::Error>,
{
    let simulation_state = SimulationState {
        nodes: Arc::clone(&runner.nodes),
    };

    let inner = runner.inner.clone();
    let nodes = runner.nodes.clone();
    let nodes_remaining: BTreeSet<NodeId> = (0..nodes.read().len()).map(From::from).collect();
    let iterations: Vec<_> = (0..maximum_iterations).collect();
    let (stop_tx, stop_rx) = bounded(1);
    let handle = SimulationRunnerHandle {
        handle: std::thread::spawn(move || {
            let p = P::new(runner.stream_settings.settings)?;
            scopeguard::defer!(if let Err(e) = p.stop() {
                eprintln!("Error stopping producer: {e}");
            });
            let subscriber = p.subscribe()?;
            std::thread::spawn(move || {
                if let Err(e) = subscriber.run() {
                    eprintln!("Error in subscriber: {e}");
                }
            });

            let mut inner = inner.write();

            'main: for chunk in iterations.chunks(update_rate) {
                select! {
                    recv(stop_rx) -> _ => break 'main,
                    default => {
                        for _ in chunk {
                            if nodes_remaining.is_empty() {
                                break 'main;
                            }

                            let node_id = *nodes_remaining.iter().choose(&mut inner.rng).expect(
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
                            if inner.check_wards(&simulation_state) {
                                // we break the outer main loop, so we need to dump it before the breaking
                                p.send(<P::Subscriber as Subscriber>::Record::try_from(
                                    &simulation_state,
                                )?)?;
                                break 'main;
                            }
                        }
                        // update_rate iterations reached, so dump state
                        p.send(<P::Subscriber as Subscriber>::Record::try_from(
                            &simulation_state,
                        )?)?;
                    }
                }
            }
            Ok(())
        }),
        stop_tx,
    };
    Ok(handle)
}
