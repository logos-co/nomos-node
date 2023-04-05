use crate::node::{Node, NodeId};
use crate::output_processors::OutData;
use crate::overlay::Overlay;
use crate::runner::SimulationRunner;
use crate::storage::StateCache;
use crate::warding::SimulationState;
use rand::prelude::SliceRandom;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;

pub fn simulate<M, N: Node, O: Overlay, S: StateCache<N::State>>(
    runner: &mut SimulationRunner<M, N, O, S>,
    chunk_size: usize,
    mut out_data: Option<&mut Vec<OutData>>,
) -> anyhow::Result<()>
where
    M: Clone,
    N::Settings: Clone,
    N: Send + Sync,
    N::State: Clone + Serialize,
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
