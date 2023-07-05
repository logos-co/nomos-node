// std
use anyhow::Ok;
use serde::Serialize;
use simulations::node::carnot::CarnotSettings;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
// crates
use clap::Parser;
use consensus_engine::overlay::{FlatOverlay, RandomBeaconState, RoundRobin};
use consensus_engine::Block;
use crossbeam::channel;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use serde::de::DeserializeOwned;
use simulations::network::behaviour::create_behaviours;
use simulations::network::regions::{create_regions, RegionsData};
use simulations::network::{InMemoryNetworkInterface, Network};
use simulations::node::{Node, NodeId};
use simulations::output_processors::Record;
use simulations::runner::SimulationRunnerHandle;
use simulations::streaming::{
    io::IOSubscriber, naive::NaiveSubscriber, polars::PolarsSubscriber, StreamType,
};
// internal
use simulations::{
    node::carnot::CarnotNode, output_processors::OutData, runner::SimulationRunner,
    settings::SimulationSettings, util::node_id,
};
mod log;

/// Main simulation wrapper
/// Pipes together the cli arguments with the execution
#[derive(Parser)]
pub struct SimulationApp {
    /// Json file path, on `SimulationSettings` format
    #[clap(long, short)]
    input_settings: PathBuf,
    #[clap(long)]
    stream_type: Option<StreamType>,
    #[clap(long, default_value = "plain")]
    log_format: log::LogFormat,
    #[clap(long, default_value = "stdout")]
    log_to: log::LogOutput,
}

impl SimulationApp {
    pub fn run(self) -> anyhow::Result<()> {
        let Self {
            input_settings,
            stream_type,
            log_format: _,
            log_to: _,
        } = self;
        let simulation_settings: SimulationSettings = load_json_from_file(&input_settings)?;

        let seed = simulation_settings.seed.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs()
        });
        let mut rng = SmallRng::seed_from_u64(seed);
        let mut node_ids: Vec<NodeId> = (0..simulation_settings.node_count).map(node_id).collect();
        node_ids.shuffle(&mut rng);

        let regions = create_regions(&node_ids, &mut rng, &simulation_settings.network_settings);
        let behaviours = create_behaviours(&simulation_settings.network_settings);
        let regions_data = RegionsData::new(regions, behaviours);

        let ids = node_ids.clone();
        let mut network = Network::new(regions_data);
        let nodes = node_ids
            .iter()
            .copied()
            .map(|node_id| {
                let (node_message_sender, node_message_receiver) = channel::unbounded();
                let network_message_receiver = network.connect(node_id, node_message_receiver);
                let network_interface = InMemoryNetworkInterface::new(
                    node_id,
                    node_message_sender,
                    network_message_receiver,
                );
                let nodes: Vec<NodeId> = ids.clone().into_iter().map(Into::into).collect();
                let leader = nodes.first().copied().unwrap();
                let overlay_settings = consensus_engine::overlay::Settings {
                    nodes: nodes.to_vec(),
                    leader: RoundRobin::new(),
                    leader_super_majority_threshold: None,
                };
                // FIXME: Actually use a proposer and a key to generate random beacon state
                let genesis = nomos_core::block::Block::new(
                    0,
                    Block::genesis().parent_qc,
                    [].into_iter(),
                    leader,
                    RandomBeaconState::Sad {
                        entropy: Box::new([0; 32]),
                    },
                );
                CarnotNode::<FlatOverlay<RoundRobin>>::new(
                    node_id,
                    CarnotSettings::new(
                        simulation_settings.node_settings.timeout,
                        simulation_settings.record_settings.clone(),
                    ),
                    overlay_settings,
                    genesis,
                    network_interface,
                    &mut rng,
                )
            })
            .collect();
        run(network, nodes, simulation_settings, stream_type)?;
        Ok(())
    }
}

fn run<M, N: Node>(
    network: Network<M>,
    nodes: Vec<N>,
    settings: SimulationSettings,
    stream_type: Option<StreamType>,
) -> anyhow::Result<()>
where
    M: Clone + Send + Sync + 'static,
    N: Send + Sync + 'static,
    N::Settings: Clone + Send,
    N::State: Serialize,
{
    let stream_settings = settings.stream_settings.clone();
    let runner =
        SimulationRunner::<_, _, OutData>::new(network, nodes, Default::default(), settings)?;

    let handle = match stream_type {
        Some(StreamType::Naive) => {
            let settings = stream_settings.unwrap_naive();
            runner.simulate_and_subscribe::<NaiveSubscriber<OutData>>(settings)?
        }
        Some(StreamType::IO) => {
            let settings = stream_settings.unwrap_io();
            runner.simulate_and_subscribe::<IOSubscriber<OutData>>(settings)?
        }
        Some(StreamType::Polars) => {
            let settings = stream_settings.unwrap_polars();
            runner.simulate_and_subscribe::<PolarsSubscriber<OutData>>(settings)?
        }
        None => runner.simulate()?,
    };

    signal(handle)
}

fn signal<R: Record>(handle: SimulationRunnerHandle<R>) -> anyhow::Result<()> {
    let (tx, rx) = crossbeam::channel::bounded(1);
    ctrlc::set_handler(move || {
        tx.send(()).unwrap();
    })?;
    loop {
        crossbeam::select! {
            recv(rx) -> _ => {
                handle.stop()?;
                tracing::info!("gracefully shutwon the simulation app");
                break;
            },
            default => {}
        }
    }
    Ok(())
}

/// Generically load a json file
fn load_json_from_file<T: DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let f = File::open(path).map_err(Box::new)?;
    Ok(serde_json::from_reader(f)?)
}

fn main() -> anyhow::Result<()> {
    let app: SimulationApp = SimulationApp::parse();
    log::config_tracing(app.log_format, &app.log_to);

    if let Err(e) = app.run() {
        tracing::error!("error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}
