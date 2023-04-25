mod async_runner;
mod glauber_runner;
mod layered_runner;
mod sync_runner;

// std
use std::sync::{Arc, RwLock};
use std::time::Duration;

// crates
use crate::streaming::{Producer, StreamSettings, Subscriber};
use crossbeam::channel::Sender;
use rand::rngs::SmallRng;
use rand::{RngCore, SeedableRng};
use rayon::prelude::*;
use serde::Serialize;

// internal
use crate::network::Network;
use crate::node::Node;
use crate::settings::{RunnerSettings, SimulationSettings};
use crate::warding::{SimulationState, SimulationWard, Ward};

pub struct SimulationRunnerHandle {
    handle: std::thread::JoinHandle<anyhow::Result<()>>,
    stop_tx: Sender<()>,
}

impl SimulationRunnerHandle {
    pub fn stop_after(self, duration: Duration) -> anyhow::Result<()> {
        std::thread::sleep(duration);
        self.stop()
    }

    pub fn stop(self) -> anyhow::Result<()> {
        if !self.handle.is_finished() {
            self.stop_tx.send(())?;
        }
        Ok(())
    }
}

pub(crate) struct SimulationRunnerInner<M> {
    network: Network<M>,
    wards: Vec<Ward>,
    rng: SmallRng,
}

impl<M> SimulationRunnerInner<M>
where
    M: Send + Sync + Clone,
{
    fn check_wards<N>(&mut self, state: &SimulationState<N>) -> bool
    where
        N: Node + Send + Sync,
        N::Settings: Clone + Send,
        N::State: Serialize,
    {
        self.wards
            .par_iter_mut()
            .map(|ward| ward.analyze(state))
            .any(|x| x)
    }

    fn step<N>(&mut self, nodes: &mut Vec<N>)
    where
        N: Node + Send + Sync,
        N::Settings: Clone + Send,
        N::State: Serialize,
    {
        self.network.dispatch_after(Duration::from_millis(100));
        nodes.par_iter_mut().for_each(|node| {
            node.step();
        });
        self.network.collect_messages();
    }
}

/// Encapsulation solution for the simulations runner
/// Holds the network state, the simulating nodes and the simulation settings.
pub struct SimulationRunner<M, N>
where
    N: Node,
{
    inner: Arc<RwLock<SimulationRunnerInner<M>>>,
    nodes: Arc<RwLock<Vec<N>>>,
    runner_settings: RunnerSettings,
    stream_settings: StreamSettings,
}

impl<M, N: Node> SimulationRunner<M, N>
where
    M: Clone + Send + Sync + 'static,
    N: Send + Sync + 'static,
    N::Settings: Clone + Send,
    N::State: Serialize,
{
    pub fn new(network: Network<M>, nodes: Vec<N>, settings: SimulationSettings) -> Self {
        let seed = settings
            .seed
            .unwrap_or_else(|| rand::thread_rng().next_u64());

        println!("Seed: {seed}");

        let rng = SmallRng::seed_from_u64(seed);
        let nodes = Arc::new(RwLock::new(nodes));
        let SimulationSettings {
            wards,
            overlay_settings: _,
            node_settings: _,
            runner_settings,
            stream_settings,
            node_count: _,
            seed: _,
            views_count: _,
            leaders_count: _,
            network_settings: _,
        } = settings;
        Self {
            inner: Arc::new(RwLock::new(SimulationRunnerInner {
                network,
                rng,
                wards,
            })),
            nodes,
            runner_settings,
            stream_settings,
        }
    }

    pub fn simulate<P: Producer>(self) -> anyhow::Result<SimulationRunnerHandle>
    where
        P::Subscriber: Send + Sync + 'static,
        <P::Subscriber as Subscriber>::Record:
            Send + Sync + 'static + for<'a> TryFrom<&'a SimulationState<N>, Error = anyhow::Error>,
    {
        match self.runner_settings.clone() {
            RunnerSettings::Sync => sync_runner::simulate::<_, _, P>(self),
            RunnerSettings::Async { chunks } => async_runner::simulate::<_, _, P>(self, chunks),
            RunnerSettings::Glauber {
                maximum_iterations,
                update_rate,
            } => glauber_runner::simulate::<_, _, P>(self, update_rate, maximum_iterations),
            RunnerSettings::Layered {
                rounds_gap,
                distribution,
            } => layered_runner::simulate::<_, _, P>(self, rounds_gap, distribution),
        }
    }
}
