pub mod carnot;

// std
use std::{
    ops::{Deref, DerefMut},
    time::Duration,
};
// crates
use rand::Rng;
use serde::{Deserialize, Serialize};
// internal

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(usize);

impl NodeId {
    #[inline]
    pub const fn new(id: usize) -> Self {
        Self(id)
    }

    #[inline]
    pub const fn inner(&self) -> usize {
        self.0
    }
}

impl From<usize> for NodeId {
    fn from(id: usize) -> Self {
        Self(id)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommitteeId(usize);

impl CommitteeId {
    #[inline]
    pub const fn new(id: usize) -> Self {
        Self(id)
    }
}

impl From<usize> for CommitteeId {
    fn from(id: usize) -> Self {
        Self(id)
    }
}

#[serde_with::serde_as]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StepTime(#[serde_as(as = "serde_with::DurationMilliSeconds")] Duration);

impl From<Duration> for StepTime {
    fn from(duration: Duration) -> Self {
        Self(duration)
    }
}

impl StepTime {
    #[inline]
    pub const fn new(duration: Duration) -> Self {
        Self(duration)
    }

    #[inline]
    pub const fn into_inner(&self) -> Duration {
        self.0
    }

    #[inline]
    pub const fn from_millis(millis: u64) -> Self {
        Self(Duration::from_millis(millis))
    }

    #[inline]
    pub const fn from_secs(secs: u64) -> Self {
        Self(Duration::from_secs(secs))
    }
}

impl Deref for StepTime {
    type Target = Duration;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for StepTime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl core::iter::Sum<Self> for StepTime {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Self(iter.into_iter().map(|s| s.0).sum())
    }
}

impl core::iter::Sum<Duration> for StepTime {
    fn sum<I: Iterator<Item = Duration>>(iter: I) -> Self {
        Self(iter.into_iter().sum())
    }
}

impl core::iter::Sum<StepTime> for Duration {
    fn sum<I: Iterator<Item = StepTime>>(iter: I) -> Self {
        iter.into_iter().map(|s| s.0).sum()
    }
}

pub trait Node {
    type Settings;
    type State;
    fn new<R: Rng>(rng: &mut R, id: NodeId, settings: Self::Settings) -> Self;
    fn id(&self) -> NodeId;
    // TODO: View must be view whenever we integrate consensus engine
    fn current_view(&self) -> usize;
    fn state(&self) -> &Self::State;
    fn step(&mut self);
}

#[cfg(test)]
impl Node for usize {
    type Settings = ();
    type State = Self;

    fn new<R: rand::Rng>(_rng: &mut R, id: NodeId, _settings: Self::Settings) -> Self {
        id.inner()
    }

    fn id(&self) -> NodeId {
        (*self).into()
    }

    fn current_view(&self) -> usize {
        *self
    }

    fn state(&self) -> &Self::State {
        self
    }

    fn step(&mut self) {
        use std::ops::AddAssign;
        self.add_assign(1);
    }
}
