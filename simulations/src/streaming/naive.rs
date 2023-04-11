use std::{
    cell::RefCell,
    fs::{File, OpenOptions},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use super::{Producer, Subscriber};

use crossbeam::channel::{bounded, unbounded, Receiver, Sender};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct NaiveSettings {
    pub path: PathBuf,
}

#[derive(Debug)]
struct Receivers<R> {
    stop_rx: Receiver<()>,
    recv: Receiver<R>,
}

#[derive(Debug)]
pub struct NaiveProducer<R> {
    sender: Sender<R>,
    stop_tx: Sender<()>,
    recvs: RefCell<Option<Receivers<R>>>,
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
            recvs: RefCell::new(Some(Receivers { stop_rx, recv })),
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
        let mut recvs = self.recvs.borrow_mut();
        if recvs.is_none() {
            return Err(anyhow::anyhow!("Producer has been subscribed"));
        }

        let mut opts = OpenOptions::new();
        let Receivers { stop_rx, recv } = recvs.take().unwrap();
        Ok(NaiveSubscriber {
            file: Arc::new(Mutex::new(
                opts.truncate(true)
                    .read(true)
                    .write(true)
                    .open(&self.settings.path)?,
            )),
            stop_rx,
            recv,
        })
    }

    fn stop(&self) -> anyhow::Result<()> {
        Ok(self.stop_tx.send(())?)
    }
}

#[derive(Debug)]
pub struct NaiveSubscriber<R> {
    file: Arc<Mutex<File>>,
    stop_rx: Receiver<()>,
    recv: Receiver<R>,
}

impl<R> Subscriber for NaiveSubscriber<R>
where
    R: Serialize + Send + Sync + 'static,
{
    type Record = R;

    fn next(&self) -> Option<anyhow::Result<Self::Record>> {
        Some(self.recv.recv().map_err(From::from))
    }

    fn run(self) -> anyhow::Result<()> {
        loop {
            crossbeam::select! {
                recv(self.stop_rx) -> _ => {
                    break;
                }
                recv(self.recv) -> msg => {
                    self.sink(msg?)?;
                }
            }
        }

        Ok(())
    }

    fn sink(&self, state: Self::Record) -> anyhow::Result<()> {
        let mut file = self.file.lock().expect("failed to lock file");
        serde_json::to_writer(&mut *file, &state)?;
        Ok(())
    }
}
