use crate::node::{Node, NodeId};
use crate::runner::SimulationRunner;
use crate::streaming::StreamProducer;
use crate::warding::SimulationState;
use crossbeam::channel::bounded;
use crossbeam::select;
use rand::prelude::SliceRandom;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;

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
    R: for<'a> TryFrom<&'a SimulationState<N>, Error = anyhow::Error> + Send + Sync + 'static,
{
    let simulation_state = SimulationState::<N> {
        nodes: Arc::clone(&runner.nodes),
    };

    let mut node_ids: Vec<NodeId> = runner
        .nodes
        .read()
        .expect("Read access to nodes vector")
        .iter()
        .map(N::id)
        .collect();

    let inner_runner = runner.inner.clone();
    let nodes = runner.nodes;
    let (stop_tx, stop_rx) = bounded(1);
    let p = StreamProducer::<R>::new();
    let p1 = p.clone();
    let handle = std::thread::spawn(move || {
        loop {
            select! {
                recv(stop_rx) -> _ => {
                    return Ok(());
                }
                default => {
                    let mut inner_runner = inner_runner.write().expect("Write access to inner in async runner");
                    node_ids.shuffle(&mut inner_runner.rng);
                    for ids_chunk in node_ids.chunks(chunk_size) {
                        let ids: HashSet<NodeId> = ids_chunk.iter().copied().collect();
                        nodes
                            .write()
                            .expect("Write access to nodes vector")
                            .par_iter_mut()
                            .filter(|n| ids.contains(&n.id()))
                            .for_each(N::step);

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
