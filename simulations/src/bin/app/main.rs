// std
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
// crates
use anyhow::Ok;
use carnot_engine::overlay::RandomBeaconState;
use carnot_engine::{Block, LeaderProof, View};
use clap::Parser;
use crossbeam::channel;
use nomos_core::block::builder::BlockBuilder;
use parking_lot::Mutex;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use serde::de::DeserializeOwned;
use serde::Serialize;
use simulations::network::behaviour::create_behaviours;
use simulations::network::regions::{create_regions, RegionsData};
use simulations::network::{InMemoryNetworkInterface, Network};
use simulations::node::carnot::{CarnotRecord, CarnotSettings, CarnotState};
use simulations::node::{NodeId, NodeIdExt};
use simulations::output_processors::Record;
use simulations::runner::{BoxedNode, SimulationRunnerHandle};
#[cfg(feature = "polars")]
use simulations::streaming::polars::PolarsSubscriber;
use simulations::streaming::{io::IOSubscriber, naive::NaiveSubscriber, StreamType};
// internal
use simulations::{runner::SimulationRunner, settings::SimulationSettings};
mod log;
mod overlay_node;

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
    #[clap(long)]
    dump_overlay_info: bool,
    #[clap(long)]
    no_netcap: bool,
}

impl SimulationApp {
    pub fn run(self) -> anyhow::Result<()> {
        let Self {
            input_settings,
            stream_type,
            log_format: _,
            log_to: _,
            dump_overlay_info,
            no_netcap,
        } = self;
        let simulation_settings: SimulationSettings = load_json_from_file(&input_settings)?;

        let seed = simulation_settings.seed.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs()
        });
        let mut rng = SmallRng::seed_from_u64(seed);
        let mut node_ids: Vec<NodeId> = (0..simulation_settings.node_count)
            .map(NodeId::from_index)
            .collect();
        node_ids.shuffle(&mut rng);

        let regions = create_regions(&node_ids, &mut rng, &simulation_settings.network_settings);
        let behaviours = create_behaviours(&simulation_settings.network_settings);
        let regions_data = RegionsData::new(regions, behaviours);

        let ids = node_ids.clone();
        let network = Arc::new(Mutex::new(Network::new(regions_data, seed)));

        if dump_overlay_info {
            dump_json_to_file(
                Path::new("overlay_info.json"),
                &overlay_node::overlay_info(
                    node_ids.clone(),
                    node_ids.first().copied().unwrap(),
                    &simulation_settings.overlay_settings,
                ),
            )?;
        }

        let nodes: Vec<BoxedNode<CarnotSettings, CarnotState>> = node_ids
            .par_iter()
            .copied()
            .map(|node_id| {
                let (node_message_broadcast_sender, node_message_broadcast_receiver) =
                    channel::unbounded();
                let (node_message_sender, node_message_receiver) = channel::unbounded();
                // Dividing milliseconds in second by milliseconds in the step.
                let step_time_as_second_fraction =
                    simulation_settings.step_time.subsec_millis() as f32 / 1_000_000_f32;
                let capacity_bps = if no_netcap {
                    None
                } else {
                    simulation_settings
                        .node_settings
                        .network_capacity_kbps
                        .map(|c| (c as f32 * 1024.0 * step_time_as_second_fraction) as u32)
                };
                let network_message_receiver = {
                    let mut network = network.lock();
                    network.connect(
                        node_id,
                        capacity_bps,
                        node_message_receiver,
                        node_message_broadcast_receiver,
                    )
                };
                let network_interface = InMemoryNetworkInterface::new(
                    node_id,
                    node_message_broadcast_sender,
                    node_message_sender,
                    network_message_receiver,
                );
                let nodes: Vec<NodeId> = ids.clone().into_iter().map(Into::into).collect();
                let leader = nodes.first().copied().unwrap();

                // FIXME: Actually use a proposer and a key to generate random beacon state
                let genesis = <BlockBuilder<_, _, (), ()>>::empty_carnot(
                    RandomBeaconState::Sad {
                        entropy: Box::new([0; 32]),
                    },
                    View::new(0),
                    Block::genesis([0; 32].into()).parent_qc,
                    LeaderProof::LeaderId {
                        leader_id: leader.into(),
                    },
                );
                let mut rng = SmallRng::seed_from_u64(seed);
                overlay_node::to_overlay_node(
                    node_id,
                    nodes,
                    leader,
                    network_interface,
                    genesis,
                    &mut rng,
                    &simulation_settings,
                )
            })
            .collect();
        let network = Arc::try_unwrap(network)
            .expect("network is not used anywhere else")
            .into_inner();
        run::<_, _, _>(network, nodes, simulation_settings, stream_type)?;
        Ok(())
    }
}

fn run<M: std::fmt::Debug, S, T>(
    network: Network<M>,
    nodes: Vec<BoxedNode<S, T>>,
    settings: SimulationSettings,
    stream_type: Option<StreamType>,
) -> anyhow::Result<()>
where
    M: Clone + Send + Sync + 'static,
    S: 'static,
    T: Serialize + Clone + 'static,
{
    let stream_settings = settings.stream_settings.clone();
    let runner = SimulationRunner::<_, CarnotRecord, S, T>::new(
        network,
        nodes,
        Default::default(),
        settings,
    )?;

    let handle = match stream_type {
        Some(StreamType::Naive) => {
            let settings = stream_settings.unwrap_naive();
            runner.simulate_and_subscribe::<NaiveSubscriber<CarnotRecord>>(settings)?
        }
        Some(StreamType::IO) => {
            let settings = stream_settings.unwrap_io();
            runner.simulate_and_subscribe::<IOSubscriber<CarnotRecord>>(settings)?
        }
        #[cfg(feature = "polars")]
        Some(StreamType::Polars) => {
            let settings = stream_settings.unwrap_polars();
            runner.simulate_and_subscribe::<PolarsSubscriber<CarnotRecord>>(settings)?
        }
        None => runner.simulate()?,
    };

    signal(handle)
}

fn signal<R: Record>(handle: SimulationRunnerHandle<R>) -> anyhow::Result<()> {
    let handle = Arc::new(handle);
    let (tx, rx) = crossbeam::channel::bounded(1);
    ctrlc::set_handler(move || {
        tx.send(()).unwrap();
    })?;
    loop {
        crossbeam::select! {
            recv(rx) -> _ => {
                handle.stop()?;
                tracing::info!("gracefully shutdown the simulation app");
                break;
            },
            default => {
                if handle.is_finished() {
                    handle.shutdown()?;
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    Ok(())
}

/// Generically load a json file
fn load_json_from_file<T: DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let f = File::open(path).map_err(Box::new)?;
    Ok(serde_json::from_reader(f)?)
}

fn dump_json_to_file<T: Serialize>(path: &Path, data: &T) -> anyhow::Result<()> {
    let f = File::create(path).map_err(Box::new)?;
    Ok(serde_json::to_writer(f, data)?)
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
