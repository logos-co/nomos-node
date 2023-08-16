use std::{
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use crossbeam::channel::{bounded, unbounded, Receiver, Sender};
use serde::{Deserialize, Serialize};

use crate::output_processors::{Record, RecordType, Runtime};

pub mod io;
pub mod naive;
pub mod polars;
pub mod runtime_subscriber;
pub mod settings_subscriber;

pub enum SubscriberType {
    Meta,
    Settings,
    Data,
}

#[derive(Debug)]
struct Receivers<R> {
    stop_rx: Receiver<Sender<()>>,
    recv: Receiver<Arc<R>>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StreamType {
    #[default]
    IO,
    Naive,
    Polars,
}

impl FromStr for StreamType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "io" => Ok(Self::IO),
            "naive" => Ok(Self::Naive),
            "polars" => Ok(Self::Polars),
            tag => Err(format!(
                "Invalid {tag} streaming type, only [naive, polars] are supported",
            )),
        }
    }
}

impl<'de> serde::Deserialize<'de> for StreamType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        StreamType::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", untagged)]
pub enum StreamSettings {
    Naive(naive::NaiveSettings),
    IO(io::IOStreamSettings),
    Polars(polars::PolarsSettings),
}

impl Default for StreamSettings {
    fn default() -> Self {
        Self::IO(Default::default())
    }
}

impl StreamSettings {
    pub fn unwrap_naive(self) -> naive::NaiveSettings {
        match self {
            StreamSettings::Naive(settings) => settings,
            _ => panic!("unwrap naive failed"),
        }
    }

    pub fn unwrap_io(self) -> io::IOStreamSettings {
        match self {
            StreamSettings::IO(settings) => settings,
            _ => panic!("unwrap io failed"),
        }
    }

    pub fn unwrap_polars(self) -> polars::PolarsSettings {
        match self {
            StreamSettings::Polars(settings) => settings,
            _ => panic!("unwrap polars failed"),
        }
    }
}

pub struct SubscriberHandle<S> {
    handle: Option<std::thread::JoinHandle<anyhow::Result<()>>>,
    stop_tx: Sender<Sender<()>>,
    subscriber: Option<S>,
}

impl<S> SubscriberHandle<S>
where
    S: Subscriber + Send + 'static,
{
    pub fn run(&mut self) {
        if self.handle.is_some() {
            return;
        }

        // unwrap safe here, because if handld is none, then we must have not booted the subscriber.
        let subscriber = self.subscriber.take().unwrap();
        let handle = std::thread::spawn(move || subscriber.run());
        self.handle = Some(handle);
    }

    pub fn stop_after(self, duration: Duration) -> anyhow::Result<()> {
        std::thread::sleep(duration);
        self.stop()
    }

    pub fn stop(self) -> anyhow::Result<()> {
        if let Some(handle) = self.handle {
            // if we have a handle, and the handle is not finished
            if !handle.is_finished() {
                let (finish_tx, finish_rx) = bounded(1);
                self.stop_tx.send(finish_tx)?;
                finish_rx.recv()?;
            } else {
                // we are sure the handle is finished, so we can join it and try to get the result.
                // if we have any error on subscriber side, return the error.
                match handle.join() {
                    Ok(rst) => rst?,
                    Err(_) => {
                        tracing::error!("Error joining subscriber thread");
                    }
                }
            }
            Ok(())
        } else {
            // if we do not have a handle, then we have not booted the subscriber yet.
            // we can just return immediately
            Ok(())
        }
    }
}

#[derive(Debug)]
struct Senders<R> {
    record_ty: RecordType,
    record_sender: Sender<Arc<R>>,
    stop_sender: Sender<Sender<()>>,
}

#[derive(Debug)]
struct StreamProducerInner<R> {
    /// senders is used to send messages to subscribers.
    senders: Vec<Senders<R>>,

    /// record_cache is used to cache messsages when there are no subscribers.
    record_cache: Vec<Arc<R>>,
}

impl<R> Default for StreamProducerInner<R> {
    fn default() -> Self {
        Self {
            senders: Vec::new(),
            record_cache: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct StreamProducer<R> {
    inner: Arc<Mutex<StreamProducerInner<R>>>,
}

impl<R> Default for StreamProducer<R> {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(StreamProducerInner::default())),
        }
    }
}

impl<R> Clone for StreamProducer<R> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<R> StreamProducer<R> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<R> StreamProducer<R>
where
    R: Record + Send + Sync + 'static,
{
    pub fn send(&self, record: R) -> anyhow::Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if inner.senders.is_empty() {
            inner.record_cache.push(Arc::new(record));
            Ok(())
        } else {
            let record = Arc::new(record);
            // cache record for new subscriber
            inner.record_cache.push(record.clone());

            // if a send fails, then it means the corresponding subscriber is dropped,
            // we just remove the sender from the list of senders.
            inner.senders.retain(|tx| {
                if tx.record_ty != record.record_type() {
                    true
                } else {
                    tx.record_sender.send(Arc::clone(&record)).is_ok()
                }
            });
            Ok(())
        }
    }

    pub fn subscribe<S: Subscriber<Record = R>>(
        &self,
        settings: S::Settings,
    ) -> anyhow::Result<SubscriberHandle<S>> {
        let (tx, rx) = unbounded();
        let (stop_tx, stop_rx) = bounded(1);
        let mut inner = self.inner.lock().unwrap();

        // send all previous records to the new subscriber
        for record in inner.record_cache.iter() {
            if S::subscribe_data_type() == record.record_type() {
                tx.send(Arc::clone(record))?;
            }
        }

        inner.senders.push(Senders {
            record_sender: tx,
            stop_sender: stop_tx.clone(),
            record_ty: S::subscribe_data_type(),
        });
        Ok(SubscriberHandle {
            handle: None,
            stop_tx,
            subscriber: Some(S::new(rx, stop_rx, settings)?),
        })
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        let meta_record = Arc::new(R::from(Runtime::load()?));
        let inner = self.inner.lock().unwrap();

        // send runtime record to runtime subscribers
        inner.senders.iter().for_each(|tx| {
            if tx.record_ty == meta_record.record_type() {
                if let Err(e) = tx.record_sender.send(Arc::clone(&meta_record)) {
                    tracing::error!("Error sending meta record: {e}");
                }
            }
        });

        // send stop signal to all subscribers
        inner.senders.iter().for_each(|tx| {
            let (finish_tx, finish_rx) = bounded(1);
            if let Err(e) = tx.stop_sender.send(finish_tx) {
                tracing::error!("Error stopping subscriber: {e}");
            } else if let Err(e) = finish_rx.recv() {
                tracing::error!("Error finilizing subscriber: {e}");
            }
        });
        Ok(())
    }
}

pub trait Subscriber {
    type Settings;
    type Record: crate::output_processors::Record + Serialize;

    fn new(
        record_recv: Receiver<Arc<Self::Record>>,
        stop_recv: Receiver<Sender<()>>,
        settings: Self::Settings,
    ) -> anyhow::Result<Self>
    where
        Self: Sized;

    fn next(&self) -> Option<anyhow::Result<Arc<Self::Record>>>;

    fn run(self) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        while let Some(state) = self.next() {
            self.sink(state?)?;
        }
        Ok(())
    }

    fn sink(&self, state: Arc<Self::Record>) -> anyhow::Result<()>;

    fn subscribe_data_type() -> RecordType;
}
