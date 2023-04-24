// std
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
// crates
use clap::Parser;
use crossbeam::channel;
use polars::io::SerWriter;
use polars::prelude::{DataFrame, JsonReader, SerReader};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use simulations::network::behaviour::NetworkBehaviour;
use simulations::network::regions::RegionsData;
use simulations::network::{InMemoryNetworkInterface, Network};
use simulations::node::dummy::DummyNode;
use simulations::node::{Node, NodeId, OverlayState, SimulationOverlay, ViewOverlay};
use simulations::overlay::flat::FlatOverlay;
use simulations::overlay::tree::TreeOverlay;
use simulations::overlay::Overlay;
use simulations::streaming::StreamType;
// internal
use simulations::{
    node::carnot::CarnotNode, output_processors::OutData, runner::SimulationRunner,
    settings::SimulationSettings, streaming::io::IOProducer, streaming::naive::NaiveProducer,
    streaming::polars::PolarsProducer,
};

/// Main simulation wrapper
/// Pipes together the cli arguments with the execution
#[derive(Parser)]
pub struct SimulationApp {
    /// Json file path, on `SimulationSettings` format
    #[clap(long, short)]
    input_settings: PathBuf,
    #[clap(long)]
    stream_type: StreamType,
}

pub enum OverlayVariants {
    Flat(FlatOverlay),
    Tree(TreeOverlay),
}

impl SimulationApp {
    pub fn run(self) -> anyhow::Result<()> {
        let Self {
            input_settings,
            stream_type,
        } = self;
        let mut rng = SmallRng::seed_from_u64(1234);
        let simulation_settings: SimulationSettings = load_json_from_file(&input_settings)?;
        let mut node_ids: Vec<NodeId> = (0..simulation_settings.node_count)
            .map(Into::into)
            .collect();
        node_ids.shuffle(&mut rng);

        let mut region_nodes = node_ids.clone();
        let regions = simulation_settings
            .regions
            .iter()
            .map(|(region, distribution)| {
                // Node ids should be already shuffled.
                let node_count = (node_ids.len() as f32 * distribution).round() as usize;
                let nodes = region_nodes.drain(..node_count).collect::<Vec<_>>();
                (*region, nodes)
            })
            .collect();
        let behaviours = simulation_settings
            .network_behaviors
            .iter()
            .map(|((a, b), d)| ((*a, *b), NetworkBehaviour::new(*d, 0.0)))
            .collect();
        let regions_data = RegionsData::new(regions, behaviours);

        let overlay: SimulationOverlay = match &simulation_settings.overlay_settings {
            simulations::settings::OverlaySettings::Flat => {
                SimulationOverlay::Flat(FlatOverlay::new())
            }
            simulations::settings::OverlaySettings::Tree(settings) => {
                SimulationOverlay::Tree(TreeOverlay::new(settings.clone()))
            }
        };

        let overlays = generate_overlays(
            &node_ids,
            &overlay,
            simulation_settings.views_count,
            simulation_settings.leaders_count,
            &mut rng,
        );

        let mut network = Network::new(regions_data);
        let nodes = node_ids
            .iter()
            .map(|node_id| {
                let node = match &simulation_settings.node_settings {
                    simulations::settings::NodeSettings::Carnot => CarnotNode::new(*node_id),
                    simulations::settings::NodeSettings::Dummy => {
                        let (node_message_sender, node_message_receiver) = channel::unbounded();
                        let network_message_receiver =
                            network.connect(*node_id, node_message_receiver);
                        let network_interface = InMemoryNetworkInterface::new(
                            *node_id,
                            node_message_sender,
                            network_message_receiver,
                        );
                        DummyNode::new(*node_id, 0, overlay_state, network_interface)
                    }
                };
                node
            })
            .collect();

        let mut simulation_runner: SimulationRunner<_, _> =
            SimulationRunner::new(network, nodes, simulation_settings);
        // build up series vector
        match stream_type {
            simulations::streaming::StreamType::Naive => {
                simulation_runner.simulate::<NaiveProducer<OutData>>()?
            }
            simulations::streaming::StreamType::Polars => {
                simulation_runner.simulate::<PolarsProducer<OutData>>()?
            }
            simulations::streaming::StreamType::IO => {
                simulation_runner.simulate::<IOProducer<std::io::Stdout, OutData>>()?
            }
        };
        Ok(())
    }
}

/// Generically load a json file
fn load_json_from_file<T: DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let f = File::open(path).map_err(Box::new)?;
    Ok(serde_json::from_reader(f)?)
}

fn generate_overlays<R: Rng>(
    node_ids: &[NodeId],
    overlay: &SimulationOverlay,
    overlay_count: usize,
    leader_count: usize,
    rng: &mut R,
) -> BTreeMap<usize, ViewOverlay> {
    (0..overlay_count)
        .map(|view_id| {
            (
                view_id,
                ViewOverlay {
                    leaders: overlay.leaders(node_ids, leader_count, rng).collect(),
                    layout: overlay.layout(node_ids, rng),
                },
            )
        })
        .collect()
}

fn main() -> anyhow::Result<()> {
    let app: SimulationApp = SimulationApp::parse();
    app.run()?;
    Ok(())
}
