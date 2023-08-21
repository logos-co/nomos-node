use super::{Receivers, StreamSettings, SubscriberFormat};
use crate::output_processors::{RecordType, Runtime};
use crossbeam::channel::{Receiver, Sender};
use parking_lot::Mutex;
use polars::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Cursor,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolarsSettings {
    pub format: SubscriberFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
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
    data: Mutex<Vec<Arc<R>>>,
    path: PathBuf,
    format: SubscriberFormat,
    recvs: Receivers<R>,
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
            SubscriberFormat::Json => dump_dataframe_to_json(&mut data, self.path.as_path()),
            SubscriberFormat::Csv => dump_dataframe_to_csv(&mut data, self.path.as_path()),
            SubscriberFormat::Parquet => dump_dataframe_to_parquet(&mut data, self.path.as_path()),
        }
    }
}

impl<R> super::Subscriber for PolarsSubscriber<R>
where
    R: crate::output_processors::Record + Serialize,
{
    type Record = R;
    type Settings = PolarsSettings;

    fn new(
        record_recv: Receiver<Arc<Self::Record>>,
        stop_recv: Receiver<Sender<()>>,
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
            data: Mutex::new(Vec::new()),
            recvs,
            path: settings.path.clone().unwrap_or_else(|| {
                let mut p = std::env::temp_dir().join("polars");
                match settings.format {
                    SubscriberFormat::Json => p.set_extension("json"),
                    SubscriberFormat::Csv => p.set_extension("csv"),
                    SubscriberFormat::Parquet => p.set_extension("parquet"),
                };
                p
            }),
            format: settings.format,
        };
        tracing::info!(
            target = "simulation",
            "subscribed to {}",
            this.path.display()
        );
        Ok(this)
    }

    fn next(&self) -> Option<anyhow::Result<Arc<Self::Record>>> {
        Some(self.recvs.recv.recv().map_err(From::from))
    }

    fn run(self) -> anyhow::Result<()> {
        loop {
            crossbeam::select! {
                recv(self.recvs.stop_rx) -> finish_tx => {
                    // Flush remaining messages after stop signal.
                    while let Ok(msg) = self.recvs.recv.try_recv() {
                        self.sink(msg)?;
                    }

                    // collect the run time meta
                    self.sink(Arc::new(R::from(Runtime::load()?)))?;

                    finish_tx?.send(())?;
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

    fn subscribe_data_type() -> RecordType {
        RecordType::Data
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
