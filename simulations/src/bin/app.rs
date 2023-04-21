// std
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
// crates
use clap::Parser;
use polars::io::SerWriter;
use polars::prelude::{DataFrame, JsonReader, SerReader};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use simulations::network::behaviour::NetworkBehaviour;
use simulations::network::regions::RegionsData;
use simulations::network::Network;
use simulations::node::{NodeId, OverlayState};
use simulations::overlay::flat::FlatOverlay;
use simulations::overlay::tree::TreeOverlay;
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
        let overlay = match simulation_settings.overlay_settings.clone() {
            simulations::settings::OverlaySettings::Flat => (),
            simulations::settings::OverlaySettings::Tree(settings) => settings,
        };
        let overlays = generate_overlays(
            &node_ids,
            overlay,
            simulation_settings.overlay_count,
            simulation_settings.leader_count,
            &mut rng,
        );
        let overlay = match simulation_settings.overlay_settings {
            simulations::settings::OverlaySettings::Flat => Box::new(FlatOverlay::new(())),
            simulations::settings::OverlaySettings::Tree(settings) => {
                Box::new(TreeOverlay::new(settings))
            }
        };
        let overlay_state = Arc::new(RwLock::new(OverlayState {
            all_nodes: node_ids.clone(),
            overlay: todo!(),
            // TODO: Generate views on demand.
            overlays,
        }));

        let network = Network::new(regions_data);
        let nodes = node_ids
            .iter()
            .map(|node_id| {
                let (node_message_sender, node_message_receiver) = channel::unbounded();
                let network_message_receiver = network.connect(*node_id, node_message_receiver);
                let network_interface = InMemoryNetworkInterface::new(
                    *node_id,
                    node_message_sender,
                    network_message_receiver,
                );
                (
                    *node_id,
                    N::new(*node_id, 0, overlay_state.clone(), network_interface),
                )
            })
            .collect();

        // build up series vector
        match stream_type {
            simulations::streaming::StreamType::Naive => {
                let simulation_settings: SimulationSettings = load_json_from_file(&input_settings)?;
                let simulation_runner: SimulationRunner<(), CarnotNode> =
                    SimulationRunner::new(network, nodes, simulation_settings);
                simulation_runner.simulate::<NaiveProducer<OutData>>()?
            }
            simulations::streaming::StreamType::Polars => {
                let simulation_settings: SimulationSettings = load_json_from_file(&input_settings)?;
                let simulation_runner: SimulationRunner<(), CarnotNode> =
                    SimulationRunner::new(network, nodes, simulation_settings);
                simulation_runner.simulate::<PolarsProducer<OutData>>()?
            }
            simulations::streaming::StreamType::IO => {
                let simulation_settings: SimulationSettings = load_json_from_file(&input_settings)?;
                let simulation_runner: SimulationRunner<(), CarnotNode> =
                    SimulationRunner::new(network, nodes, simulation_settings);
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

fn main() -> anyhow::Result<()> {
    let app: SimulationApp = SimulationApp::parse();
    app.run()?;
    Ok(())
}
