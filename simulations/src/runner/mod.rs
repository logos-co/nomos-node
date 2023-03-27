mod async_runner;
mod glauber_runner;
mod layered_runner;
mod sync_runner;

use std::marker::PhantomData;
// std
use std::sync::{Arc, RwLock};
// crates
use rand::rngs::SmallRng;
use rand::{RngCore, SeedableRng};
use rayon::prelude::*;
// internal
use crate::node::{NetworkState, Node, NodeId, SharedState};
use crate::output_processors::OutData;
use crate::overlay::Overlay;
use crate::settings::{RunnerSettings, SimulationSettings};
use crate::warding::{SimulationState, SimulationWard};

/// Encapsulation solution for the simulations runner
/// Holds the network state, the simulating nodes and the simulation settings.
pub struct SimulationRunner<N, O>
where
    N: Node,
    O: Overlay,
{
    nodes: Arc<RwLock<Vec<N>>>,
    settings: SimulationSettings<N::Settings, O::Settings>,
    rng: SmallRng,
    _overlay: PhantomData<O>,
}

impl<N: Node, O: Overlay> SimulationRunner<N, O>
where
    N: Send + Sync,
    N::Settings: Clone,
    O::Settings: Clone,
{
    pub fn new(settings: SimulationSettings<N::Settings, O::Settings>) -> Self {
        let seed = settings
            .seed
            .unwrap_or_else(|| rand::thread_rng().next_u64());

        println!("Seed: {seed}");

        let mut rng = SmallRng::seed_from_u64(seed);
        let overlay = O::new(settings.overlay_settings.clone());
        let nodes = Self::nodes_from_initial_settings(&settings, overlay, &mut rng);

        let nodes = Arc::new(RwLock::new(nodes));

        Self {
            nodes,
            settings,
            rng,
            _overlay: Default::default(),
        }
    }

    /// Initialize nodes from settings and calculate initial network state.
    fn nodes_from_initial_settings(
        settings: &SimulationSettings<N::Settings, O::Settings>,
        overlay: O,
        seed: &mut SmallRng,
    ) -> Vec<N> {
        let SimulationSettings {
            node_settings,
            node_count,
            ..
        } = settings;

        let node_ids: Vec<NodeId> = (0..*node_count).map(Into::into).collect();
        let network_state: SharedState<NetworkState> = Arc::new(RwLock::new(NetworkState {
            layout: overlay.layout(&node_ids, seed),
        }));

        node_ids
            .iter()
            .map(|id| N::new(seed, *id, node_settings.clone(), network_state.clone()))
            .collect()
    }

    pub fn simulate(&mut self, out_data: Option<&mut Vec<OutData>>) {
        match self.settings.runner_settings.clone() {
            RunnerSettings::Sync => {
                sync_runner::simulate(self, out_data);
            }
            RunnerSettings::Async { chunks } => {
                async_runner::simulate(self, chunks, out_data);
            }
            RunnerSettings::Glauber {
                maximum_iterations,
                update_rate,
            } => {
                glauber_runner::simulate(self, update_rate, maximum_iterations, out_data);
            }
            RunnerSettings::Layered {
                rounds_gap,
                distribution,
            } => {
                layered_runner::simulate(self, rounds_gap, distribution, out_data);
            }
        }
    }

    fn dump_state_to_out_data(
        &self,
        _simulation_state: &SimulationState<N>,
        _out_data: &mut Option<&mut Vec<OutData>>,
    ) {
        todo!("What data do we want to expose?")
    }

    fn check_wards(&mut self, state: &SimulationState<N>) -> bool {
        self.settings
            .wards
            .par_iter_mut()
            .map(|ward| ward.analyze(state))
            .any(|x| x)
    }

    fn step(&mut self) {
        self.nodes
            .write()
            .expect("Single access to nodes vector")
            .par_iter_mut()
            .for_each(|node| {
                node.step();
            });
    }
}
