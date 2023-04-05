mod async_runner;
mod glauber_runner;
mod layered_runner;
mod sync_runner;

// std
use std::marker::PhantomData;

use std::sync::{Arc, RwLock};
use std::time::Duration;
// crates
use crate::network::Network;
use crate::storage::StateCache;
use rand::rngs::SmallRng;
use rand::{RngCore, SeedableRng};
use rayon::prelude::*;
use serde::Serialize;
// internal
use crate::node::Node;
use crate::output_processors::OutData;
use crate::overlay::Overlay;
use crate::settings::{RunnerSettings, SimulationSettings};
use crate::warding::{SimulationState, SimulationWard};

/// Encapsulation solution for the simulations runner
/// Holds the network state, the simulating nodes and the simulation settings.
pub struct SimulationRunner<M, N, O, S>
where
    N: Node,
    O: Overlay,
    S: StateCache<N::State>,
{
    nodes: Arc<RwLock<Vec<N>>>,
    state_cache: Arc<RwLock<S>>,
    network: Network<M>,
    settings: SimulationSettings<N::Settings, O::Settings>,
    rng: SmallRng,
    _overlay: PhantomData<O>,
}

impl<M, N: Node, O: Overlay, S> SimulationRunner<M, N, O, S>
where
    M: Clone,
    N: Send + Sync,
    N::Settings: Clone,
    N::State: Serialize + Clone,
    O::Settings: Clone,
    S: StateCache<N::State>,
{
    pub fn new(
        network: Network<M>,
        nodes: Vec<N>,
        settings: SimulationSettings<N::Settings, O::Settings>,
    ) -> Self {
        let seed = settings
            .seed
            .unwrap_or_else(|| rand::thread_rng().next_u64());

        println!("Seed: {seed}");

        let rng = SmallRng::seed_from_u64(seed);
        let cache = S::new(&nodes);
        let nodes = Arc::new(RwLock::new(nodes));

        Self {
            nodes,
            network,
            settings,
            rng,
            _overlay: Default::default(),
            state_cache: Arc::new(RwLock::new(cache)),
        }
    }

    pub fn simulate(&mut self, out_data: Option<&mut Vec<OutData>>) -> anyhow::Result<()> {
        match self.settings.runner_settings.clone() {
            RunnerSettings::Sync => sync_runner::simulate(self, out_data),
            RunnerSettings::Async { chunks } => async_runner::simulate(self, chunks, out_data),
            RunnerSettings::Glauber {
                maximum_iterations,
                update_rate,
            } => glauber_runner::simulate(self, update_rate, maximum_iterations, out_data),
            RunnerSettings::Layered {
                rounds_gap,
                distribution,
            } => layered_runner::simulate(self, rounds_gap, distribution, out_data),
        }
    }

    fn dump_state_to_out_data(
        &self,
        simulation_state: &SimulationState<N>,
        out_data: &mut Option<&mut Vec<OutData>>,
    ) -> anyhow::Result<()> {
        if let Some(out_data) = out_data {
            out_data.push(OutData::try_from(simulation_state)?);
        }
        Ok(())
    }

    fn check_wards(&mut self, state: &SimulationState<N>) -> bool {
        self.settings
            .wards
            .par_iter_mut()
            .map(|ward| ward.analyze(state))
            .any(|x| x)
    }

    fn step(&mut self) {
        self.network
            .dispatch_after(&mut self.rng, Duration::from_millis(100));
        let mut nodes = self.nodes.write().expect("Single access to nodes vector");

        nodes.par_iter_mut().for_each(|node| {
            node.step();
        });
        self.state_cache
            .write()
            .expect("Single access to state cache")
            .update_many(&nodes);
        self.network.collect_messages();
    }
}
