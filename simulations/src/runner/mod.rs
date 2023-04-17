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
use crossbeam::channel::{Sender, Receiver, bounded};
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

struct SimulationRunnerInner<M, N, O>
where
    N: Node,
    O: Overlay,
{
    nodes: Arc<RwLock<Vec<N>>>,
    network: Network<M>,
    rng: SmallRng,
    settings: SimulationSettings<N::Settings, O::Settings>,
    _overlay: PhantomData<O>,
}

impl<M, N: Node, O: Overlay> SimulationRunnerInner<M, N, O>
where
    M: Clone + Send,
    N: Send + Sync,
    N::Settings: Clone + Send,
    N::State: Serialize,
    O: Send,
    O::Settings: Clone + Send,
{
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

fn dump_state_to_out_data<N>(
    simulation_state: &SimulationState<N>,
    out_data: &mut Option<&mut Vec<OutData>>,
) -> anyhow::Result<()> 
where
    N: Node,
    N: Send + Sync,
    N::Settings: Clone + Send,
    N::State: Serialize,
{
    if let Some(out_data) = out_data {
        out_data.push(OutData::try_from(simulation_state)?);
    }
    Ok(())
}

/// Encapsulation solution for the simulations runner
/// Holds the network state, the simulating nodes and the simulation settings.
pub struct SimulationRunner<M, N, O>
where
    N: Node,
    O: Overlay,
{
    inner: Arc<RwLock<SimulationRunnerInner<M, N, O>>>,
    stop_rx: Option<Receiver<()>>,
}

impl<M, N: Node, O: Overlay> SimulationRunner<M, N, O>
where
    M: Clone + Send + Sync,
    N: Send + Sync,
    N::Settings: Clone + Send,
    N::State: Serialize,
    O: Send + Sync,
    O::Settings: Clone + Send + Sync,
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
        let nodes = Arc::new(RwLock::new(nodes));
        Self {
            inner: Arc::new(RwLock::new(
                SimulationRunnerInner {
                    nodes,
                    network,
                    rng,
                    _overlay: PhantomData,
                    settings,
                }
            )),
            stop_rx: None,
        }
    }

    pub fn simulate(&mut self, out_data: Option<&mut Vec<OutData>>) -> anyhow::Result<Sender<()>> {
        let (tx, rx) = bounded(1);
        self.stop_rx = Some(rx);
        let inner = self.inner.clone();
        std::thread::spawn(|| {
            let mut inner = inner.write().expect("Single access to simulation runner inner");
            match inner.settings.runner_settings.clone() {
                RunnerSettings::Sync => sync_runner::simulate(&mut inner, out_data),
                RunnerSettings::Async { chunks } => async_runner::simulate(&mut inner, chunks, out_data),
                RunnerSettings::Glauber {
                    maximum_iterations,
                    update_rate,
                } => glauber_runner::simulate(&mut inner, update_rate, maximum_iterations, out_data),
                RunnerSettings::Layered {
                    rounds_gap,
                    distribution,
                } => layered_runner::simulate(&mut inner, rounds_gap, distribution, out_data),
            }
        });
        Ok(tx)
    }

    fn check_wards(&mut self, state: &SimulationState<N>) -> bool {
        self.settings
            .wards
            .par_iter_mut()
            .map(|ward| ward.analyze(state))
            .any(|x| x)
    }

    fn step(&self) {
        self.inner.write().expect("Single access to simulation runner inner").step();
    }
}


