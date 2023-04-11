use crate::node::{Node, NodeId};
use crate::output_processors::OutData;
use crate::overlay::Overlay;
use crate::runner::SimulationRunner;
use crate::streaming::{Producer, Subscriber};
use crate::warding::SimulationState;
use rand::prelude::SliceRandom;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;

pub fn simulate<M, N: Node, O: Overlay>(
    runner: &mut SimulationRunner<M, N, O>,
    chunk_size: usize,
    mut out_data: Option<&mut Vec<OutData>>,
) -> anyhow::Result<()>
where
    M: Clone,
    N::Settings: Clone,
    N: Send + Sync,
    N::State: Serialize,
    O::Settings: Clone,
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

    runner.dump_state_to_out_data(&simulation_state, &mut out_data)?;

    loop {
        node_ids.shuffle(&mut runner.rng);
        for ids_chunk in node_ids.chunks(chunk_size) {
            let ids: HashSet<NodeId> = ids_chunk.iter().copied().collect();
            runner
                .nodes
                .write()
                .expect("Write access to nodes vector")
                .par_iter_mut()
                .filter(|n| ids.contains(&n.id()))
                .for_each(N::step);

            runner.dump_state_to_out_data(&simulation_state, &mut out_data)?;
        }
        // check if any condition makes the simulation stop
        if runner.check_wards(&simulation_state) {
            break;
        }
    }
    Ok(())
}

/// Simulate with sending the network state to any subscriber
pub fn simulate_with_subscriber<M, N: Node, O: Overlay, P: Producer>(
    runner: &mut SimulationRunner<M, N, O>,
    chunk_size: usize,
    settings: P::Settings,
) -> anyhow::Result<()>
where
    M: Clone,
    N: Send + Sync,
    N::Settings: Clone,
    N::State: Serialize,
    O::Settings: Clone,
    P::Subscriber: Send + Sync + 'static,
    <P::Subscriber as Subscriber>::Record:
        for<'a> TryFrom<&'a SimulationState<N>, Error = anyhow::Error>,
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

    loop {
        node_ids.shuffle(&mut runner.rng);
        for ids_chunk in node_ids.chunks(chunk_size) {
            let ids: HashSet<NodeId> = ids_chunk.iter().copied().collect();
            runner
                .nodes
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
        if runner.check_wards(&simulation_state) {
            break;
        }
    }
    Ok(())
}
