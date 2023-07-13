use std::{any::Any, io::stdout, sync::Arc};

use super::{Receivers, StreamSettings, Subscriber};
use crate::output_processors::{RecordType, Runtime};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IOStreamSettings {
    #[serde(rename = "type")]
    pub writer_type: WriteType,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteType {
    #[default]
    Stdout,
}

pub trait ToWriter<W: std::io::Write + Send + Sync + 'static> {
    fn to_writer(&self) -> anyhow::Result<W>;
}

impl<W: std::io::Write + Send + Sync + 'static> ToWriter<W> for WriteType {
    fn to_writer(&self) -> anyhow::Result<W> {
        match self {
            WriteType::Stdout => {
                let boxed_any = Box::new(stdout()) as Box<dyn Any + Send + Sync>;
                Ok(boxed_any.downcast::<W>().map(|boxed| *boxed).map_err(|_| {
                    std::io::Error::new(std::io::ErrorKind::Other, "Writer type mismatch")
                })?)
            }
        }
    }
}

impl TryFrom<StreamSettings> for IOStreamSettings {
    type Error = String;

    fn try_from(settings: StreamSettings) -> Result<Self, Self::Error> {
        match settings {
            StreamSettings::IO(settings) => Ok(settings),
            _ => Err("io settings can't be created".into()),
        }
    }
}

#[derive(Debug)]
pub struct IOSubscriber<R, W = std::io::Stdout> {
    recvs: Arc<Receivers<R>>,
    writer: Arc<Mutex<W>>,
}

impl<W, R> Subscriber for IOSubscriber<R, W>
where
    W: std::io::Write + Send + Sync + 'static,
    R: crate::output_processors::Record + Serialize,
{
    type Record = R;
    type Settings = IOStreamSettings;

    fn new(
        record_recv: crossbeam::channel::Receiver<Arc<Self::Record>>,
        stop_recv: crossbeam::channel::Receiver<()>,
        settings: Self::Settings,
    ) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            recvs: Arc::new(Receivers {
                stop_rx: stop_recv,
                recv: record_recv,
            }),
            writer: Arc::new(Mutex::new(settings.writer_type.to_writer()?)),
        })
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
        serde_json::to_writer(&mut *self.writer.lock(), &state)?;
        Ok(())
    }

    fn subscribe_data_type() -> RecordType {
        RecordType::Data
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use crate::{
        network::{
            behaviour::NetworkBehaviour,
            regions::{Region, RegionsData},
            Network, NetworkBehaviourKey,
        },
        node::{dummy_streaming::DummyStreamingNode, Node, NodeId, NodeIdExt},
        output_processors::OutData,
        runner::SimulationRunner,
        warding::SimulationState,
    };

    use super::*;
    #[derive(Debug, Clone, Serialize)]
    struct IORecord {
        states: HashMap<NodeId, usize>,
    }

    impl TryFrom<&SimulationState> for IORecord {
        type Error = anyhow::Error;

        fn try_from(value: &SimulationState) -> Result<Self, Self::Error> {
            let nodes = value.nodes.read();
            Ok(Self {
                states: nodes
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
            .map(|idx| DummyStreamingNode::new(NodeId::from_index(idx), ()))
            .collect::<Vec<_>>();
        let network = Network::new(RegionsData {
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
        });
        let simulation_runner: SimulationRunner<(), DummyStreamingNode<()>, OutData> =
            SimulationRunner::new(network, nodes, Default::default(), simulation_settings).unwrap();
        simulation_runner
            .simulate()
            .unwrap()
            .stop_after(Duration::from_millis(100))
            .unwrap();
    }
}
