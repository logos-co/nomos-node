use super::{Receivers, Subscriber};
use crate::output_processors::{RecordType, Runtime};
use crossbeam::channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSettings {
    pub path: PathBuf,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        let mut tmp = std::env::temp_dir();
        tmp.push("simulation");
        tmp.set_extension("runtime");
        Self { path: tmp }
    }
}

#[derive(Debug)]
pub struct RuntimeSubscriber<R> {
    file: Arc<Mutex<File>>,
    recvs: Arc<Receivers<R>>,
}

impl<R> Subscriber for RuntimeSubscriber<R>
where
    R: crate::output_processors::Record + Serialize,
{
    type Record = R;

    type Settings = RuntimeSettings;

    fn new(
        record_recv: Receiver<Arc<Self::Record>>,
        stop_recv: Receiver<Sender<()>>,
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
        let this = RuntimeSubscriber {
            file: Arc::new(Mutex::new(
                opts.truncate(true)
                    .create(true)
                    .read(true)
                    .write(true)
                    .open(&settings.path)?,
            )),
            recvs: Arc::new(recvs),
        };
        tracing::info!(
            taget = "simulation",
            "subscribed to {}",
            settings.path.display()
        );
        Ok(this)
    }

    fn next(&self) -> Option<anyhow::Result<Arc<Self::Record>>> {
        Some(self.recvs.recv.recv().map_err(From::from))
    }

    fn run(self) -> anyhow::Result<()> {
        crossbeam::select! {
            recv(self.recvs.stop_rx) -> finish_tx => {
                // collect the run time meta
                self.sink(Arc::new(R::from(Runtime::load()?)))?;
                finish_tx?.send(())?;
            }
            recv(self.recvs.recv) -> msg => {
                self.sink(msg?)?;
            }
        }

        Ok(())
    }

    fn sink(&self, state: Arc<Self::Record>) -> anyhow::Result<()> {
        let mut file = self.file.lock().expect("failed to lock file");
        serde_json::to_writer(&mut *file, &state)?;
        file.write_all(b",\n")?;
        Ok(())
    }

    fn subscribe_data_type() -> RecordType {
        RecordType::Data
    }
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
    struct RuntimeRecord {
        states: HashMap<NodeId, View>,
    }

    impl<S, T: Serialize> TryFrom<&SimulationState<S, T>> for RuntimeRecord {
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
