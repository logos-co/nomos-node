use parking_lot::Mutex;
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Cursor,
    path::{Path, PathBuf},
    str::FromStr,
};

use super::{Receivers, StreamSettings};

#[derive(Debug, Clone, Copy, Serialize)]
pub enum PolarsFormat {
    Json,
    Csv,
    Parquet,
}

impl FromStr for PolarsFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "csv" => Ok(Self::Csv),
            "parquet" => Ok(Self::Parquet),
            tag => Err(format!(
                "Invalid {tag} format, only [json, csv, parquet] are supported",
            )),
        }
    }
}

impl<'de> Deserialize<'de> for PolarsFormat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        PolarsFormat::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolarsSettings {
    pub format: PolarsFormat,
    pub path: PathBuf,
}

impl TryFrom<StreamSettings> for PolarsSettings {
    type Error = String;

    fn try_from(settings: StreamSettings) -> Result<Self, Self::Error> {
        match settings {
            StreamSettings::Polars(settings) => Ok(settings),
            _ => Err("polars settings can't be created".into()),
        }
    }
}

#[derive(Debug)]
pub struct PolarsSubscriber<R> {
    data: Arc<Mutex<Vec<Arc<R>>>>,
    path: PathBuf,
    format: PolarsFormat,
    recvs: Arc<Receivers<R>>,
}

impl<R> PolarsSubscriber<R>
where
    R: Serialize,
{
    fn persist(&self) -> anyhow::Result<()> {
        let data = self.data.lock();
        let mut cursor = Cursor::new(Vec::new());
        serde_json::to_writer(&mut cursor, &*data).expect("Dump data to json ");
        let mut data = JsonReader::new(cursor)
            .finish()
            .expect("Load dataframe from intermediary json");

        data.unnest(["state"])?;
        match self.format {
            PolarsFormat::Json => dump_dataframe_to_json(&mut data, self.path.as_path()),
            PolarsFormat::Csv => dump_dataframe_to_csv(&mut data, self.path.as_path()),
            PolarsFormat::Parquet => dump_dataframe_to_parquet(&mut data, self.path.as_path()),
        }
    }
}

impl<R> super::Subscriber for PolarsSubscriber<R>
where
    R: Serialize + Send + Sync + 'static,
{
    type Record = R;
    type Settings = PolarsSettings;

    fn new(
        record_recv: crossbeam::channel::Receiver<Arc<Self::Record>>,
        stop_recv: crossbeam::channel::Receiver<()>,
        settings: Self::Settings,
    ) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let recvs = Receivers {
            stop_rx: stop_recv,
            recv: record_recv,
        };
        let this = PolarsSubscriber {
            data: Arc::new(Mutex::new(Vec::new())),
            recvs: Arc::new(recvs),
            path: settings.path.clone(),
            format: settings.format,
        };
        Ok(this)
    }

    fn next(&self) -> Option<anyhow::Result<Arc<Self::Record>>> {
        Some(self.recvs.recv.recv().map_err(From::from))
    }

    fn run(self) -> anyhow::Result<()> {
        loop {
            crossbeam::select! {
                recv(self.recvs.stop_rx) -> _ => {
                    return self.persist();
                }
                recv(self.recvs.recv) -> msg => {
                    self.sink(msg?)?;
                }
            }
        }
    }

    fn sink(&self, state: Arc<Self::Record>) -> anyhow::Result<()> {
        self.data.lock().push(state);
        Ok(())
    }
}

fn dump_dataframe_to_json(data: &mut DataFrame, out_path: &Path) -> anyhow::Result<()> {
    let out_path = out_path.with_extension("json");
    let f = File::create(out_path)?;
    let mut writer = polars::prelude::JsonWriter::new(f);
    Ok(writer.finish(data)?)
}

fn dump_dataframe_to_csv(data: &mut DataFrame, out_path: &Path) -> anyhow::Result<()> {
    let out_path = out_path.with_extension("csv");
    let f = File::create(out_path)?;
    let mut writer = polars::prelude::CsvWriter::new(f);
    Ok(writer.finish(data)?)
}

fn dump_dataframe_to_parquet(data: &mut DataFrame, out_path: &Path) -> anyhow::Result<()> {
    let out_path = out_path.with_extension("parquet");
    let f = File::create(out_path)?;
    let writer = polars::prelude::ParquetWriter::new(f);
    Ok(writer.finish(data).map(|_| ())?)
}
