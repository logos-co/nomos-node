use std::sync::{Arc, Mutex};

use super::{Receivers, Subscriber};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct IOStreamSettings<W = std::io::Stdout> {
    pub writer: W,
}

impl Default for IOStreamSettings {
    fn default() -> Self {
        Self {
            writer: std::io::stdout(),
        }
    }
}

impl<'de> Deserialize<'de> for IOStreamSettings {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self {
            writer: std::io::stdout(),
        })
    }
}

#[derive(Debug)]
pub struct IOSubscriber<W, R> {
    recvs: Arc<Receivers<R>>,
    writer: Arc<Mutex<W>>,
}

impl<W, R> Subscriber for IOSubscriber<W, R>
where
    W: std::io::Write + Send + Sync + 'static,
    R: Serialize + Send + Sync + 'static,
{
    type Record = R;
    type Settings = IOStreamSettings<W>;

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
            writer: Arc::new(Mutex::new(settings.writer)),
        })
    }

    fn next(&self) -> Option<anyhow::Result<Arc<Self::Record>>> {
        Some(self.recvs.recv.recv().map_err(From::from))
    }

    fn run(self) -> anyhow::Result<()> {
        loop {
            crossbeam::select! {
                recv(self.recvs.stop_rx) -> _ => {
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
        serde_json::to_writer(
            &mut *self
                .writer
                .lock()
                .expect("fail to lock writer in io subscriber"),
            &state,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use crate::{
        network::{
            behaviour::NetworkBehaviour,
            regions::{Region, RegionsData},
            Network,
        },
        node::{dummy_streaming::DummyStreamingNode, Node, NodeId},
        output_processors::OutData,
        overlay::tree::TreeOverlay,
        runner::SimulationRunner,
        warding::SimulationState,
    };

    use super::*;
    #[derive(Debug, Clone, Serialize)]
    struct IORecord {
        states: HashMap<NodeId, usize>,
    }

    impl TryFrom<&SimulationState<DummyStreamingNode<()>>> for IORecord {
        type Error = anyhow::Error;

        fn try_from(value: &SimulationState<DummyStreamingNode<()>>) -> Result<Self, Self::Error> {
            let nodes = value.nodes.read().expect("failed to read nodes");
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
            .map(|idx| DummyStreamingNode::new(NodeId::from(idx), ()))
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
                    (region, vec![idx.into()])
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
                    (idx.into(), region)
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
                        (region, region),
                        NetworkBehaviour {
                            delay: Duration::from_millis(100),
                            drop: 0.0,
                        },
                    )
                })
                .collect(),
        });
        let simulation_runner: SimulationRunner<(), DummyStreamingNode<()>, TreeOverlay, OutData> =
            SimulationRunner::new(network, nodes, simulation_settings);
        simulation_runner
            .simulate()
            .unwrap()
            .stop_after(Duration::from_millis(100))
            .unwrap();
    }
}
