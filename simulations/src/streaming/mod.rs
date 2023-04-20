use std::str::FromStr;

use crossbeam::channel::Receiver;
use serde::{Deserialize, Serialize};

pub mod io;
pub mod naive;
pub mod polars;

#[derive(Debug)]
struct Receivers<R> {
    stop_rx: Receiver<()>,
    recv: Receiver<R>,
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

#[derive(Debug, Deserialize)]
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

pub trait Producer: Send + Sync + 'static {
    type Settings: Send;
    type Subscriber: Subscriber;

    fn new(settings: StreamSettings) -> anyhow::Result<Self>
    where
        Self: Sized;

    fn send(&self, state: <Self::Subscriber as Subscriber>::Record) -> anyhow::Result<()>;

    fn subscribe(&self) -> anyhow::Result<Self::Subscriber>
    where
        Self::Subscriber: Sized;

    fn stop(&self) -> anyhow::Result<()>;
}

pub trait Subscriber {
    type Record: Serialize + Send + Sync + 'static;

    fn next(&self) -> Option<anyhow::Result<Self::Record>>;

    fn run(self) -> anyhow::Result<()>
    where
        Self: Sized,
    {
        while let Some(state) = self.next() {
            self.sink(state?)?;
        }
        Ok(())
    }

    fn sink(&self, state: Self::Record) -> anyhow::Result<()>;
}
