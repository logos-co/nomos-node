use std::collections::HashMap;
// std
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::str::FromStr;
// crates
use clap::Parser;
use polars::io::SerWriter;
use polars::prelude::{DataFrame, JsonReader, SerReader};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use simulations::network::regions::RegionsData;
use simulations::network::Network;
use simulations::overlay::tree::TreeOverlay;
// internal
use simulations::{
    node::carnot::CarnotNode, output_processors::OutData, runner::SimulationRunner,
    settings::SimulationSettings,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
enum OutputType {
    File(PathBuf),
    StdOut,
    StdErr,
}

impl core::fmt::Display for OutputType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputType::File(path) => write!(f, "{}", path.display()),
            OutputType::StdOut => write!(f, "stdout"),
            OutputType::StdErr => write!(f, "stderr"),
        }
    }
}

/// Output format selector enum
#[derive(Clone, Debug, Default)]
enum OutputFormat {
    Json,
    Csv,
    #[default]
    Parquet,
}

impl Display for OutputFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let tag = match self {
            OutputFormat::Json => "json",
            OutputFormat::Csv => "csv",
            OutputFormat::Parquet => "parquet",
        };
        write!(f, "{tag}")
    }
}

impl FromStr for OutputFormat {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "csv" => Ok(Self::Csv),
            "parquet" => Ok(Self::Parquet),
            tag => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid {tag} tag, only [json, csv, polars] are supported",),
            )),
        }
    }
}

/// Main simulation wrapper
/// Pipes together the cli arguments with the execution
#[derive(Parser)]
pub struct SimulationApp {
    /// Json file path, on `SimulationSettings` format
    #[clap(long, short)]
    input_settings: PathBuf,
    /// Output file path
    #[clap(long, short)]
    output_file: PathBuf,
    /// Output format selector
    #[clap(long, short = 'f', default_value_t)]
    output_format: OutputFormat,
}

impl SimulationApp {
    pub fn run(self) -> Result<(), Box<dyn Error>> {
        let Self {
            input_settings,
            output_file,
            output_format,
        } = self;
        let simulation_settings: SimulationSettings<_, _> = load_json_from_file(&input_settings)?;
        let nodes = vec![]; // TODO: Initialize nodes of different types.
        let regions_data = RegionsData::new(HashMap::new(), HashMap::new());
        let network = Network::new(regions_data);

        let mut simulation_runner: SimulationRunner<(), CarnotNode, TreeOverlay> =
            SimulationRunner::new(network, nodes, simulation_settings);
        // build up series vector
        let mut out_data: Vec<OutData> = Vec::new();
        simulation_runner.simulate(Some(&mut out_data));
        let mut dataframe: DataFrame = out_data_to_dataframe(out_data);
        dump_dataframe_to(output_format, &mut dataframe, &output_file)?;
        Ok(())
    }
}

fn out_data_to_dataframe(out_data: Vec<OutData>) -> DataFrame {
    let mut cursor = Cursor::new(Vec::new());
    serde_json::to_writer(&mut cursor, &out_data).expect("Dump data to json ");
    let dataframe = JsonReader::new(cursor)
        .finish()
        .expect("Load dataframe from intermediary json");

    dataframe
        .unnest(["state"])
        .expect("Node state should be unnest")
}

/// Generically load a json file
fn load_json_from_file<T: DeserializeOwned>(path: &Path) -> Result<T, Box<dyn Error>> {
    let f = File::open(path).map_err(Box::new)?;
    serde_json::from_reader(f).map_err(|e| Box::new(e) as Box<dyn Error>)
}

fn dump_dataframe_to_json(data: &mut DataFrame, out_path: &Path) -> Result<(), Box<dyn Error>> {
    let out_path = out_path.with_extension("json");
    let f = File::create(out_path)?;
    let mut writer = polars::prelude::JsonWriter::new(f);
    writer
        .finish(data)
        .map_err(|e| Box::new(e) as Box<dyn Error>)
}

fn dump_dataframe_to_csv(data: &mut DataFrame, out_path: &Path) -> Result<(), Box<dyn Error>> {
    let out_path = out_path.with_extension("csv");
    let f = File::create(out_path)?;
    let mut writer = polars::prelude::CsvWriter::new(f);
    writer
        .finish(data)
        .map_err(|e| Box::new(e) as Box<dyn Error>)
}

fn dump_dataframe_to_parquet(data: &mut DataFrame, out_path: &Path) -> Result<(), Box<dyn Error>> {
    let out_path = out_path.with_extension("parquet");
    let f = File::create(out_path)?;
    let writer = polars::prelude::ParquetWriter::new(f);
    writer
        .finish(data)
        .map(|_| ())
        .map_err(|e| Box::new(e) as Box<dyn Error>)
}

fn dump_dataframe_to(
    output_format: OutputFormat,
    data: &mut DataFrame,
    out_path: &Path,
) -> Result<(), Box<dyn Error>> {
    match output_format {
        OutputFormat::Json => dump_dataframe_to_json(data, out_path),
        OutputFormat::Csv => dump_dataframe_to_csv(data, out_path),
        OutputFormat::Parquet => dump_dataframe_to_parquet(data, out_path),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let app: SimulationApp = SimulationApp::parse();
    app.run()?;
    Ok(())
}
