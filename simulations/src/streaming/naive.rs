use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Arc,
};

use super::{Producer, Receivers, Subscriber};
use arc_swap::ArcSwapOption;
use crossbeam::channel::{bounded, unbounded, Sender};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NaiveSettings {
    pub path: PathBuf,
}

impl Default for NaiveSettings {
    fn default() -> Self {
        let mut tmp = std::env::temp_dir();
        tmp.push("simulation");
        tmp.set_extension("data");
        Self { path: tmp }
    }
}

#[derive(Debug)]
pub struct NaiveProducer<R> {
    sender: Sender<R>,
    stop_tx: Sender<()>,
    recvs: ArcSwapOption<Receivers<R>>,
    settings: NaiveSettings,
}

impl<R> Producer for NaiveProducer<R>
where
    R: Serialize + Send + Sync + 'static,
{
    type Settings = NaiveSettings;

    type Subscriber = NaiveSubscriber<R>;

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
            settings,
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

        let mut opts = OpenOptions::new();
        let recvs = self.recvs.swap(None).unwrap();
        let this = NaiveSubscriber {
            file: Arc::new(Mutex::new(
                opts.truncate(true)
                    .create(true)
                    .read(true)
                    .write(true)
                    .open(&self.settings.path)?,
            )),
            recvs,
        };
        eprintln!("Subscribed to {}", self.settings.path.display());
        Ok(this)
    }

    fn stop(&self) -> anyhow::Result<()> {
        Ok(self.stop_tx.send(())?)
    }
}

#[derive(Debug)]
pub struct NaiveSubscriber<R> {
    file: Arc<Mutex<File>>,
    recvs: Arc<Receivers<R>>,
}

impl<R> Subscriber for NaiveSubscriber<R>
where
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
        let mut file = self.file.lock();
        serde_json::to_writer(&mut *file, &state)?;
        file.write_all(b",\n")?;
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
        warding::SimulationState,
    };

    use super::*;
    #[derive(Debug, Clone, Serialize)]
    struct NaiveRecord {
        states: HashMap<NodeId, usize>,
    }

    impl TryFrom<&SimulationState<DummyStreamingNode<()>>> for NaiveRecord {
        type Error = anyhow::Error;

        fn try_from(value: &SimulationState<DummyStreamingNode<()>>) -> Result<Self, Self::Error> {
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
            NaiveProducer<NaiveRecord>,
        > = SimulationRunner::new(network, nodes, simulation_settings);

        simulation_runner.simulate().unwrap();
    }
}
