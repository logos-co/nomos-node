use std::sync::Arc;

use super::{Producer, Receivers, Subscriber};
use arc_swap::ArcSwapOption;
use crossbeam::channel::{bounded, unbounded, Sender};
use parking_lot::Mutex;
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
pub struct IOProducer<W, R> {
    sender: Sender<R>,
    stop_tx: Sender<()>,
    recvs: ArcSwapOption<Receivers<R>>,
    writer: ArcSwapOption<Mutex<W>>,
}

impl<W, R> Producer for IOProducer<W, R>
where
    W: std::io::Write + Send + Sync + 'static,
    R: Serialize + Send + Sync + 'static,
{
    type Settings = IOStreamSettings<W>;

    type Subscriber = IOSubscriber<W, R>;

    fn new(settings: Self::Settings) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let (sender, recv) = unbounded();
        let (stop_tx, stop_rx) = bounded(1);
        Ok(Self {
            sender,
            recvs: ArcSwapOption::from(Some(Arc::new(Receivers { stop_rx, recv }))),
            stop_tx,
            writer: ArcSwapOption::from(Some(Arc::new(Mutex::new(settings.writer)))),
        })
    }

    fn send(&self, state: <Self::Subscriber as Subscriber>::Record) -> anyhow::Result<()> {
        self.sender.send(state).map_err(From::from)
    }

    fn subscribe(&self) -> anyhow::Result<Self::Subscriber>
    where
        Self::Subscriber: Sized,
    {
        let recvs = self.recvs.load();
        if recvs.is_none() {
            return Err(anyhow::anyhow!("Producer has been subscribed"));
        }

        let recvs = self.recvs.swap(None).unwrap();
        let writer = self.writer.swap(None).unwrap();
        let this = IOSubscriber { recvs, writer };
        Ok(this)
    }

    fn stop(&self) -> anyhow::Result<()> {
        Ok(self.stop_tx.send(())?)
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

    fn next(&self) -> Option<anyhow::Result<Self::Record>> {
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

    fn sink(&self, state: Self::Record) -> anyhow::Result<()> {
        serde_json::to_writer(&mut *self.writer.lock(), &state)?;
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
        overlay::tree::TreeOverlay,
        runner::SimulationRunner,
        streaming::{StreamSettings, StreamType},
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
            stream_settings: StreamSettings {
                ty: StreamType::IO,
                settings: IOStreamSettings {
                    writer: std::io::stdout(),
                },
            },
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
        let simulation_runner: SimulationRunner<
            (),
            DummyStreamingNode<()>,
            TreeOverlay,
            IOProducer<std::io::Stdout, IORecord>,
        > = SimulationRunner::new(network, nodes, simulation_settings);
        simulation_runner
            .simulate()
            .unwrap()
            .stop_after(Duration::from_millis(100))
            .unwrap();
    }
}
