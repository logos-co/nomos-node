use crate::node::{Node, NodeId};
use crate::runner::{SimulationRunner, SimulationRunnerHandle};
use crate::streaming::{Producer, Subscriber};
use crate::warding::SimulationState;
use crossbeam::channel::bounded;
use crossbeam::select;
use rand::prelude::SliceRandom;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;

/// Simulate with sending the network state to any subscriber
pub fn simulate<M, N: Node, P: Producer>(
    runner: SimulationRunner<M, N>,
    chunk_size: usize,
) -> anyhow::Result<SimulationRunnerHandle>
where
    M: Clone + Send + Sync + 'static,
    N: Send + Sync + 'static,
    N::Settings: Clone + Send,
    N::State: Serialize,
    P::Subscriber: Send + Sync + 'static,
    <P::Subscriber as Subscriber>::Record:
        Send + Sync + 'static + for<'a> TryFrom<&'a SimulationState<N>, Error = anyhow::Error>,
{
    let simulation_state = SimulationState::<N> {
        nodes: Arc::clone(&runner.nodes),
    };

    let mut node_ids: Vec<NodeId> = runner.nodes.read().iter().map(N::id).collect();

    let inner = runner.inner.clone();
    let nodes = runner.nodes.clone();
    let (stop_tx, stop_rx) = bounded(1);
    let handle = SimulationRunnerHandle {
        stop_tx,
        handle: std::thread::spawn(move || {
            let p = P::new(runner.stream_settings)?;
            scopeguard::defer!(if let Err(e) = p.stop() {
                eprintln!("Error stopping producer: {e}");
            });
            let subscriber = p.subscribe()?;
            std::thread::spawn(move || {
                if let Err(e) = subscriber.run() {
                    eprintln!("Error in subscriber: {e}");
                }
            });
            loop {
                select! {
                    recv(stop_rx) -> _ => {
                        return Ok(());
                    }
                    default => {
                        let mut inner = inner.write();
                        node_ids.shuffle(&mut inner.rng);
                        for ids_chunk in node_ids.chunks(chunk_size) {
                            let ids: HashSet<NodeId> = ids_chunk.iter().copied().collect();
                            nodes
                                .write()
                                .par_iter_mut()
                                .filter(|n| ids.contains(&n.id()))
                                .for_each(N::step);

                            p.send(<P::Subscriber as Subscriber>::Record::try_from(
                                &simulation_state,
                            )?)?;
                        }
                        // check if any condition makes the simulation stop
                        if inner.check_wards(&simulation_state) {
                            return Ok(());
                        }
                    }
                }
            }
        }),
    };
    Ok(handle)
}
