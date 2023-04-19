// std
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
// crates
use clap::Parser;
use serde::de::DeserializeOwned;
use simulations::network::regions::RegionsData;
use simulations::network::Network;
use simulations::overlay::tree::TreeOverlay;
use simulations::streaming::StreamType;
// internal
use simulations::{
    node::carnot::CarnotNode, output_processors::OutData, runner::SimulationRunner,
    settings::SimulationSettings, streaming::naive::NaiveProducer,
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

        let nodes = vec![]; // TODO: Initialize nodes of different types.
        let regions_data = RegionsData::new(HashMap::new(), HashMap::new());
        let network = Network::new(regions_data);

        // build up series vector
        match stream_type {
            simulations::streaming::StreamType::Naive => {
                let simulation_settings: SimulationSettings<_, _, _> =
                    load_json_from_file(&input_settings)?;
                let mut simulation_runner: SimulationRunner<
                    (),
                    CarnotNode,
                    TreeOverlay,
                    NaiveProducer<OutData>,
                > = SimulationRunner::new(network, nodes, simulation_settings);
                simulation_runner.simulate()?
            }
            simulations::streaming::StreamType::Polars => {
                let simulation_settings: SimulationSettings<_, _, _> =
                    load_json_from_file(&input_settings)?;
                let mut simulation_runner: SimulationRunner<
                    (),
                    CarnotNode,
                    TreeOverlay,
                    PolarsProducer<OutData>,
                > = SimulationRunner::new(network, nodes, simulation_settings);
                simulation_runner.simulate()?
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
