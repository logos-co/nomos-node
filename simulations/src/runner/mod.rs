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
use crate::streaming::{Producer, Subscriber};
use rand::rngs::SmallRng;
use rand::{RngCore, SeedableRng};
use rayon::prelude::*;
use serde::Serialize;
// internal
use crate::node::Node;
use crate::overlay::Overlay;
use crate::settings::{RunnerSettings, SimulationSettings};
use crate::warding::{SimulationState, SimulationWard};

/// Encapsulation solution for the simulations runner
/// Holds the network state, the simulating nodes and the simulation settings.
pub struct SimulationRunner<M, N, O, P>
where
    N: Node,
    O: Overlay,
    P: Producer,
{
    nodes: Arc<RwLock<Vec<N>>>,
    network: Network<M>,
    settings: SimulationSettings<N::Settings, O::Settings, P::Settings>,
    rng: SmallRng,
    _overlay: PhantomData<O>,
}

impl<M, N: Node, O: Overlay, P: Producer> SimulationRunner<M, N, O, P>
where
    M: Clone,
    N: Send + Sync,
    N::Settings: Clone,
    N::State: Serialize,
    O::Settings: Clone,
    P::Subscriber: Send + Sync + 'static,
    <P::Subscriber as Subscriber>::Record:
        Send + Sync + 'static + for<'a> TryFrom<&'a SimulationState<N>, Error = anyhow::Error>,
{
    pub fn new(
        network: Network<M>,
        nodes: Vec<N>,
        settings: SimulationSettings<N::Settings, O::Settings, P::Settings>,
    ) -> Self {
        let seed = settings
            .seed
            .unwrap_or_else(|| rand::thread_rng().next_u64());

        println!("Seed: {seed}");

        let rng = SmallRng::seed_from_u64(seed);
        let nodes = Arc::new(RwLock::new(nodes));

        Self {
            nodes,
            network,
            settings,
            rng,
            _overlay: Default::default(),
        }
    }

    pub fn simulate(&mut self) -> anyhow::Result<()> {
        match self.settings.runner_settings.clone() {
            RunnerSettings::Sync => sync_runner::simulate::<_, _, _, P>(
                self,
                self.settings.stream_settings.settings.clone(),
            ),
            RunnerSettings::Async { chunks } => async_runner::simulate::<_, _, _, P>(
                self,
                chunks,
                self.settings.stream_settings.settings.clone(),
            ),
            RunnerSettings::Glauber {
                maximum_iterations,
                update_rate,
            } => glauber_runner::simulate::<_, _, _, P>(
                self,
                update_rate,
                maximum_iterations,
                self.settings.stream_settings.settings.clone(),
            ),
            RunnerSettings::Layered {
                rounds_gap,
                distribution,
            } => layered_runner::simulate::<_, _, _, P>(
                self,
                rounds_gap,
                distribution,
                self.settings.stream_settings.settings.clone(),
            ),
        }
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
        self.nodes
            .write()
            .expect("Single access to nodes vector")
            .par_iter_mut()
            .for_each(|node| {
                node.step();
            });
        self.network.collect_messages();
    }
}
