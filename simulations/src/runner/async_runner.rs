use crate::node::{Node, NodeId};
use crate::overlay::Overlay;
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
pub fn simulate<M, N: Node, O: Overlay, P: Producer>(
    runner: &mut SimulationRunner<M, N, O, P>,
    chunk_size: usize,
    settings: P::Settings,
) -> anyhow::Result<SimulationRunnerHandle>
where
    M: Clone + Send + Sync,
    N: Send + Sync,
    N::Settings: Clone + Send,
    N::State: Serialize,
    O::Settings: Clone,
    P::Subscriber: Send + Sync + 'static,
    <P::Subscriber as Subscriber>::Record:
        Send + Sync + 'static + for<'a> TryFrom<&'a SimulationState<N>, Error = anyhow::Error>,
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

    let p = P::new(settings)?;
    scopeguard::defer!(if let Err(e) = p.stop() {
        eprintln!("Error stopping producer: {e}");
    });
    let subscriber = p.subscribe()?;
    std::thread::spawn(move || {
        if let Err(e) = subscriber.run() {
            eprintln!("Error in subscriber: {e}");
        }
    });

    let mut inner = runner.inner.clone();
    let nodes = runner.nodes.clone();
    let (stop_tx, stop_rx) = bounded(1);
    let handle = SimulationRunnerHandle {
        stop_tx,
        handle: std::thread::spawn(move || {
            loop {
                select! {
                    recv(stop_rx) -> _ => {
                        return Ok(());
                    }
                    default => {
                        let mut inner = inner.write().expect("Write access to inner in async runner");
                        node_ids.shuffle(&mut inner.rng);
                        for ids_chunk in node_ids.chunks(chunk_size) {
                            let ids: HashSet<NodeId> = ids_chunk.iter().copied().collect();
                            nodes
                                .write()
                                .expect("Write access to nodes vector")
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
