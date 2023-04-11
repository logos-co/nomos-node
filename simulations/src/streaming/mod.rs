use serde::Serialize;

pub mod naive;

pub trait Producer {
    type Settings;
    type Subscriber: Subscriber;

    fn new(settings: Self::Settings) -> anyhow::Result<Self>
    where
        Self: Sized;

    fn send(&self, state: <Self::Subscriber as Subscriber>::Record) -> anyhow::Result<()>;

    fn subscribe(&self) -> anyhow::Result<Self::Subscriber>
    where
        Self::Subscriber: Sized;

    fn stop(&self) -> anyhow::Result<()>;
}

pub trait Subscriber {
    type Record: Serialize;

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
