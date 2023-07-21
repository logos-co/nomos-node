mod async_runner;
mod glauber_runner;
mod layered_runner;
mod sync_runner;

// std
use std::sync::Arc;
use std::time::Duration;

use crate::output_processors::Record;
// crates
use crate::streaming::{
    runtime_subscriber::RuntimeSubscriber, settings_subscriber::SettingsSubscriber, StreamProducer,
    Subscriber, SubscriberHandle,
};
use crossbeam::channel::Sender;
use parking_lot::RwLock;
use rand::rngs::SmallRng;
use rand::{RngCore, SeedableRng};
use rayon::prelude::*;
use serde::Serialize;

// internal
use crate::network::Network;
use crate::node::Node;
use crate::settings::{RunnerSettings, SimulationSettings};
use crate::warding::{SimulationState, SimulationWard, Ward};

pub type BoxedNode<S, T> = Box<dyn Node<Settings = S, State = T> + Send + Sync>;

pub struct SimulationRunnerHandle<R> {
    producer: StreamProducer<R>,
    stop_tx: Sender<()>,
    handle: std::thread::JoinHandle<anyhow::Result<()>>,
}

impl<R: Record> SimulationRunnerHandle<R> {
    pub fn stop_after(self, duration: Duration) -> anyhow::Result<()> {
        std::thread::sleep(duration);
        self.stop()
    }

    pub fn stop(self) -> anyhow::Result<()> {
        if !self.handle.is_finished() {
            self.stop_tx.send(())?;
            self.producer.stop()?;
        }
        Ok(())
    }

    pub fn subscribe<S: Subscriber<Record = R>>(
        &self,
        settings: S::Settings,
    ) -> anyhow::Result<SubscriberHandle<S>> {
        self.producer.subscribe(settings)
    }

    pub fn join(self) -> anyhow::Result<()> {
        self.handle.join().expect("Join simulation thread")
    }
}

pub(crate) struct SimulationRunnerInner<M: std::fmt::Debug> {
    network: Network<M>,
    wards: Vec<Ward>,
    rng: SmallRng,
}

impl<M> SimulationRunnerInner<M>
where
    M: std::fmt::Debug + Send + Sync + Clone,
{
    fn check_wards<S, T>(&mut self, state: &SimulationState<S, T>) -> bool {
        self.wards
            .par_iter_mut()
            .map(|ward| ward.analyze(state))
            .any(|x| x)
    }

    fn step<S, T>(&mut self, nodes: &mut [BoxedNode<S, T>], elapsed: Duration) {
        self.network.dispatch_after(elapsed);
        nodes.par_iter_mut().for_each(|node| {
            node.step(elapsed);
        });
        self.network.collect_messages();
    }
}

/// Encapsulation solution for the simulations runner
/// Holds the network state, the simulating nodes and the simulation settings.
pub struct SimulationRunner<M: std::fmt::Debug, R, S, T> {
    inner: SimulationRunnerInner<M>,
    nodes: Arc<RwLock<Vec<BoxedNode<S, T>>>>,
    runner_settings: RunnerSettings,
    producer: StreamProducer<R>,
}

impl<M, R, S, T> SimulationRunner<M, R, S, T>
where
    M: std::fmt::Debug + Clone + Send + Sync + 'static,
    R: Record
        + for<'a> TryFrom<&'a SimulationState<S, T>, Error = anyhow::Error>
        + Send
        + Sync
        + 'static,
    S: 'static,
    T: Serialize + 'static,
{
    pub fn new(
        network: Network<M>,
        nodes: Vec<BoxedNode<S, T>>,
        producer: StreamProducer<R>,
        mut settings: SimulationSettings,
    ) -> anyhow::Result<Self> {
        let seed = settings
            .seed
            .unwrap_or_else(|| rand::thread_rng().next_u64());

        settings
            .seed
            .get_or_insert_with(|| rand::thread_rng().next_u64());

        // Store the settings to the producer so that we can collect them later
        producer.send(R::from(settings.clone()))?;

        let rng = SmallRng::seed_from_u64(seed);
        let nodes = Arc::new(RwLock::new(nodes));
        let SimulationSettings {
            wards,
            overlay_settings: _,
            node_settings: _,
            runner_settings,
            stream_settings: _,
            node_count: _,
            seed: _,
            views_count: _,
            leaders_count: _,
            network_settings: _,
            step_time: _,
            record_settings: _,
        } = settings;
        Ok(Self {
            runner_settings,
            inner: SimulationRunnerInner {
                network,
                rng,
                wards,
            },
            nodes,
            producer,
        })
    }

    pub fn simulate(self) -> anyhow::Result<SimulationRunnerHandle<R>> {
        // init the start time
        let _ = *crate::START_TIME;

        match self.runner_settings.clone() {
            RunnerSettings::Sync => sync_runner::simulate(self),
            RunnerSettings::Async { chunks } => async_runner::simulate(self, chunks),
            RunnerSettings::Glauber {
                maximum_iterations,
                update_rate,
            } => glauber_runner::simulate(self, update_rate, maximum_iterations),
            RunnerSettings::Layered {
                rounds_gap,
                distribution,
            } => layered_runner::simulate(self, rounds_gap, distribution),
        }
    }
}

impl<M, R, S, T> SimulationRunner<M, R, S, T>
where
    M: std::fmt::Debug + Clone + Send + Sync + 'static,
    R: Record
        + serde::Serialize
        + for<'a> TryFrom<&'a SimulationState<S, T>, Error = anyhow::Error>
        + Send
        + Sync
        + 'static,
    S: 'static,
    T: Serialize + 'static,
{
    pub fn simulate_and_subscribe<B>(
        self,
        settings: B::Settings,
    ) -> anyhow::Result<SimulationRunnerHandle<R>>
    where
        B: Subscriber<Record = R> + Send + Sync + 'static,
    {
        let handle = self.simulate()?;
        let mut data_subscriber_handle = handle.subscribe::<B>(settings)?;
        let mut runtime_subscriber_handle =
            handle.subscribe::<RuntimeSubscriber<R>>(Default::default())?;
        let mut settings_subscriber_handle =
            handle.subscribe::<SettingsSubscriber<R>>(Default::default())?;
        std::thread::scope(|s| {
            s.spawn(move || {
                data_subscriber_handle.run();
            });

            s.spawn(move || {
                runtime_subscriber_handle.run();
            });

            s.spawn(move || {
                settings_subscriber_handle.run();
            });
        });

        Ok(handle)
    }
}
