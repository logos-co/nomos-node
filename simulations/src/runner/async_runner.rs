use crate::node::{Node, NodeId};
use crate::output_processors::OutData;
use crate::overlay::Overlay;
use crate::runner::SimulationRunner;
use crate::warding::SimulationState;
use rand::prelude::SliceRandom;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;

use super::{SimulationRunnerInner, dump_state_to_out_data};

pub fn simulate<M, N: Node, O: Overlay>(
    runner: &mut SimulationRunnerInner<M, N, O>,
    chunk_size: usize,
    mut out_data: Option<&mut Vec<OutData>>,
) -> anyhow::Result<()>
where
    M: Clone + Send,
    N: Send + Sync,
    N::Settings: Clone + Send,
    N::State: Serialize,
    O: Send,
    O::Settings: Clone + Send,
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

    dump_state_to_out_data(&simulation_state, &mut out_data)?;

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

            dump_state_to_out_data(&simulation_state, &mut out_data)?;
        }
        // check if any condition makes the simulation stop
        if runner.check_wards(&simulation_state) {
            break;
        }
    }
    Ok(())
}
