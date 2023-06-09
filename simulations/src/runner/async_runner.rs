use crate::node::{Node, NodeId};
use crate::output_processors::Record;
use crate::runner::SimulationRunner;
use crate::warding::SimulationState;
use crossbeam::channel::bounded;
use crossbeam::select;
use rand::prelude::SliceRandom;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use super::SimulationRunnerHandle;

/// Simulate with sending the network state to any subscriber
pub fn simulate<M, N: Node, R>(
    runner: SimulationRunner<M, N, R>,
    chunk_size: usize,
) -> anyhow::Result<SimulationRunnerHandle<R>>
where
    M: Clone + Send + Sync + 'static,
    N: Send + Sync + 'static,
    N::Settings: Clone + Send,
    N::State: Serialize,
    R: Record
        + for<'a> TryFrom<&'a SimulationState<N>, Error = anyhow::Error>
        + Send
        + Sync
        + 'static,
{
    let simulation_state = SimulationState::<N> {
        nodes: Arc::clone(&runner.nodes),
    };

    let mut node_ids: Vec<NodeId> = runner.nodes.read().iter().map(N::id).collect();

    let inner_runner = runner.inner.clone();
    let nodes = runner.nodes;
    let (stop_tx, stop_rx) = bounded(1);
    let p = runner.producer.clone();
    let p1 = runner.producer;
    let elapsed = Duration::from_millis(100);
    let handle = std::thread::spawn(move || {
        loop {
            select! {
                recv(stop_rx) -> _ => {
                    return Ok(());
                }
                default => {
                    let mut inner_runner = inner_runner.write();
                    node_ids.shuffle(&mut inner_runner.rng);
                    for ids_chunk in node_ids.chunks(chunk_size) {
                        let ids: HashSet<NodeId> = ids_chunk.iter().copied().collect();
                        nodes
                            .write()
                            .par_iter_mut()
                            .filter(|n| ids.contains(&n.id()))
                            .for_each(|node|node.step(elapsed));

                        p.send(R::try_from(
                            &simulation_state,
                        )?)?;
                    }
                    // check if any condition makes the simulation stop
                    if inner_runner.check_wards(&simulation_state) {
                        return Ok(());
                    }
                }
            }
        }
    });
    Ok(SimulationRunnerHandle {
        producer: p1,
        stop_tx,
        handle,
    })
}
