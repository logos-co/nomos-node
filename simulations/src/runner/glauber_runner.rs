use crate::node::{Node, NodeId};
use crate::output_processors::OutData;
use crate::overlay::Overlay;
use crate::runner::SimulationRunner;
use crate::streaming::{Producer, Subscriber};
use crate::warding::SimulationState;
use rand::prelude::IteratorRandom;
use serde::Serialize;
use std::collections::BTreeSet;
use std::sync::Arc;

/// [Glauber dynamics simulation](https://en.wikipedia.org/wiki/Glauber_dynamics)
pub fn simulate<M, N: Node, O: Overlay>(
    runner: &mut SimulationRunner<M, N, O>,
    update_rate: usize,
    maximum_iterations: usize,
    mut out_data: Option<&mut Vec<OutData>>,
) -> anyhow::Result<()>
where
    M: Clone,
    N: Send + Sync,
    N::Settings: Clone,
    N::State: Serialize,
    O::Settings: Clone,
{
    let simulation_state = SimulationState {
        nodes: Arc::clone(&runner.nodes),
    };
    let nodes_remaining: BTreeSet<NodeId> = (0..runner
        .nodes
        .read()
        .expect("Read access to nodes vector")
        .len())
        .map(From::from)
        .collect();
    let iterations: Vec<_> = (0..maximum_iterations).collect();
    'main: for chunk in iterations.chunks(update_rate) {
        for _ in chunk {
            if nodes_remaining.is_empty() {
                break 'main;
            }

            let node_id = *nodes_remaining.iter().choose(&mut runner.rng).expect(
                "Some id to be selected as it should be impossible for the set to be empty here",
            );

            {
                let mut shared_nodes = runner.nodes.write().expect("Write access to nodes vector");
                let node: &mut N = shared_nodes
                    .get_mut(node_id.inner())
                    .expect("Node should be present");
                node.step();
            }

            // check if any condition makes the simulation stop
            if runner.check_wards(&simulation_state) {
                // we break the outer main loop, so we need to dump it before the breaking
                runner.dump_state_to_out_data(&simulation_state, &mut out_data)?;
                break 'main;
            }
        }
        // update_rate iterations reached, so dump state
        runner.dump_state_to_out_data(&simulation_state, &mut out_data)?;
    }
    Ok(())
}

/// Simulate with sending the network state to any subscriber
pub fn simulate_with_subscriber<M, N: Node, O: Overlay, P: Producer>(
    runner: &mut SimulationRunner<M, N, O>,
    update_rate: usize,
    maximum_iterations: usize,
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
    let simulation_state = SimulationState {
        nodes: Arc::clone(&runner.nodes),
    };
    let p = P::new(settings)?;
    scopeguard::defer!(if let Err(e) = p.stop() {
        eprintln!("Error stopping producer: {}", e);
    });
    let subscriber = p.subscribe()?;
    std::thread::spawn(move || {
        if let Err(e) = subscriber.run() {
            eprintln!("Error in subscriber: {}", e);
        }
    });
    let nodes_remaining: BTreeSet<NodeId> = (0..runner
        .nodes
        .read()
        .expect("Read access to nodes vector")
        .len())
        .map(From::from)
        .collect();
    let iterations: Vec<_> = (0..maximum_iterations).collect();
    'main: for chunk in iterations.chunks(update_rate) {
        for _ in chunk {
            if nodes_remaining.is_empty() {
                break 'main;
            }

            let node_id = *nodes_remaining.iter().choose(&mut runner.rng).expect(
                "Some id to be selected as it should be impossible for the set to be empty here",
            );

            {
                let mut shared_nodes = runner.nodes.write().expect("Write access to nodes vector");
                let node: &mut N = shared_nodes
                    .get_mut(node_id.inner())
                    .expect("Node should be present");
                node.step();
            }

            // check if any condition makes the simulation stop
            if runner.check_wards(&simulation_state) {
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
    Ok(())
}
