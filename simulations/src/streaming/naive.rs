use super::{Receivers, StreamSettings, Subscriber, SubscriberFormat};
use crate::output_processors::{Record, RecordType, Runtime};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Seek, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NaiveSettings {
    pub path: PathBuf,
    #[serde(default = "SubscriberFormat::csv")]
    pub format: SubscriberFormat,
}

impl TryFrom<StreamSettings> for NaiveSettings {
    type Error = String;

    fn try_from(settings: StreamSettings) -> Result<Self, Self::Error> {
        match settings {
            StreamSettings::Naive(settings) => Ok(settings),
            _ => Err("naive settings can't be created".into()),
        }
    }
}

impl Default for NaiveSettings {
    fn default() -> Self {
        let mut tmp = std::env::temp_dir();
        tmp.push("simulation");
        tmp.set_extension("data");
        Self {
            path: tmp,
            format: SubscriberFormat::Csv,
        }
    }
}

#[derive(Debug)]
pub struct NaiveSubscriber<R> {
    file: Mutex<File>,
    recvs: Receivers<R>,
    initialized: AtomicBool,
    format: SubscriberFormat,
}

impl<R> Subscriber for NaiveSubscriber<R>
where
    R: crate::output_processors::Record + Serialize,
{
    type Record = R;

    type Settings = NaiveSettings;

    fn new(
        record_recv: crossbeam::channel::Receiver<Arc<Self::Record>>,
        stop_recv: crossbeam::channel::Receiver<()>,
        settings: Self::Settings,
    ) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let mut opts = OpenOptions::new();
        let recvs = Receivers {
            stop_rx: stop_recv,
            recv: record_recv,
        };
        let this = NaiveSubscriber {
            file: Mutex::new(
                opts.truncate(true)
                    .create(true)
                    .read(true)
                    .write(true)
                    .open(&settings.path)?,
            ),
            recvs,
            initialized: AtomicBool::new(false),
            format: settings.format,
        };
        tracing::info!(
            target = "simulation",
            "subscribed to {}",
            settings.path.display()
        );
        Ok(this)
    }

    fn next(&self) -> Option<anyhow::Result<Arc<Self::Record>>> {
        Some(self.recvs.recv.recv().map_err(From::from))
    }

    fn run(self) -> anyhow::Result<()> {
        loop {
            crossbeam::select! {
                recv(self.recvs.stop_rx) -> _ => {
                    // collect the run time meta
                    self.sink(Arc::new(R::from(Runtime::load()?)))?;
                    break;
                }
                recv(self.recvs.recv) -> msg => {
                    self.sink(msg?)?;
                }
            }
        }

        Ok(())
    }

    fn sink(&self, state: Arc<Self::Record>) -> anyhow::Result<()> {
        let mut file = self.file.lock();
        match self.format {
            SubscriberFormat::Json => {
                write_json_record(&mut *file, &self.initialized, &*state)?;
            }
            SubscriberFormat::Csv => {
                let mut w = csv::Writer::from_writer(&mut *file);
                write_csv_record(&mut w, &self.initialized, &*state)?;
            }
            SubscriberFormat::Parquet => {
                panic!("native subscriber does not support parquet format")
            }
        }

        Ok(())
    }

    fn subscribe_data_type() -> RecordType {
        RecordType::Data
    }
}

impl<R> Drop for NaiveSubscriber<R> {
    fn drop(&mut self) {
        if SubscriberFormat::Json == self.format {
            let mut file = self.file.lock();
            // To construct a valid json format, we need to overwrite the last comma
            if let Err(e) = file
                .seek(std::io::SeekFrom::End(-1))
                .and_then(|_| file.write_all(b"]}"))
            {
                tracing::error!(target="simulations", err=%e, "fail to close json format");
            }
        }
    }
}

fn write_json_record<W: std::io::Write, R: Record>(
    mut w: W,
    initialized: &AtomicBool,
    record: &R,
) -> std::io::Result<()> {
    if !initialized.load(Ordering::Acquire) {
        w.write_all(b"{\"records\": [")?;
    }
    let records = record.data();
    let num_records = records.len();
    for (idx, data) in records.into_iter().enumerate() {
        serde_json::to_writer(&mut w, data)?;
        if idx != num_records - 1 {
            w.write_all(b",")?;
        }
    }
    Ok(())
}

fn write_csv_record<W: std::io::Write, R: Record>(
    w: &mut csv::Writer<W>,
    initialized: &AtomicBool,
    record: &R,
) -> csv::Result<()> {
    // If have not write csv header, then write it
    if !initialized.load(Ordering::Acquire) {
        w.write_record(record.fields()).map_err(|e| {
            tracing::error!(target = "simulations", err = %e, "fail to write CSV header");
            e
        })?;

        initialized.store(true, Ordering::Release);
    }
    for data in record.data() {
        w.serialize(data).map_err(|e| {
            tracing::error!(target = "simulations", err = %e, "fail to write CSV record");
            e
        })?
    }
    w.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use consensus_engine::View;

    use crate::{
        network::{
            behaviour::NetworkBehaviour,
            regions::{Region, RegionsData},
            Network, NetworkBehaviourKey,
        },
        node::{
            dummy_streaming::{DummyStreamingNode, DummyStreamingState},
            Node, NodeId, NodeIdExt,
        },
        output_processors::OutData,
        runner::SimulationRunner,
        warding::SimulationState,
    };

    use super::*;
    #[derive(Debug, Clone, Serialize)]
    struct NaiveRecord {
        states: HashMap<NodeId, View>,
    }

    impl<S, T: Serialize> TryFrom<&SimulationState<S, T>> for NaiveRecord {
        type Error = anyhow::Error;

        fn try_from(value: &SimulationState<S, T>) -> Result<Self, Self::Error> {
            Ok(Self {
                states: value
                    .nodes
                    .read()
                    .iter()
                    .map(|node| (node.id(), node.current_view()))
                    .collect(),
            })
        }
    }

    #[test]
    fn test_streaming() {
        let simulation_settings = crate::settings::SimulationSettings {
            seed: Some(1),
            ..Default::default()
        };

        let nodes = (0..6)
            .map(|idx| {
                Box::new(DummyStreamingNode::new(NodeId::from_index(idx), ()))
                    as Box<
                        dyn Node<State = DummyStreamingState, Settings = ()>
                            + std::marker::Send
                            + Sync,
                    >
            })
            .collect::<Vec<_>>();
        let network = Network::new(
            RegionsData {
                regions: (0..6)
                    .map(|idx| {
                        let region = match idx % 6 {
                            0 => Region::Europe,
                            1 => Region::NorthAmerica,
                            2 => Region::SouthAmerica,
                            3 => Region::Asia,
                            4 => Region::Africa,
                            5 => Region::Australia,
                            _ => unreachable!(),
                        };
                        (region, vec![NodeId::from_index(idx)])
                    })
                    .collect(),
                node_region: (0..6)
                    .map(|idx| {
                        let region = match idx % 6 {
                            0 => Region::Europe,
                            1 => Region::NorthAmerica,
                            2 => Region::SouthAmerica,
                            3 => Region::Asia,
                            4 => Region::Africa,
                            5 => Region::Australia,
                            _ => unreachable!(),
                        };
                        (NodeId::from_index(idx), region)
                    })
                    .collect(),
                region_network_behaviour: (0..6)
                    .map(|idx| {
                        let region = match idx % 6 {
                            0 => Region::Europe,
                            1 => Region::NorthAmerica,
                            2 => Region::SouthAmerica,
                            3 => Region::Asia,
                            4 => Region::Africa,
                            5 => Region::Australia,
                            _ => unreachable!(),
                        };
                        (
                            NetworkBehaviourKey::new(region, region),
                            NetworkBehaviour {
                                delay: Duration::from_millis(100),
                                drop: 0.0,
                            },
                        )
                    })
                    .collect(),
            },
            0,
        );
        let simulation_runner: SimulationRunner<(), OutData, (), DummyStreamingState> =
            SimulationRunner::new(network, nodes, Default::default(), simulation_settings).unwrap();
        simulation_runner.simulate().unwrap();
    }
}
