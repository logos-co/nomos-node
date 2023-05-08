pub mod carnot;
pub mod dummy;

#[cfg(test)]
pub mod dummy_streaming;

// std
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
    sync::Arc,
    time::Duration,
};
// crates
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
// internal
use crate::overlay::Layout;

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

#[derive(Clone, Debug)]
pub struct ViewOverlay {
    pub leaders: Vec<NodeId>,
    pub layout: Layout,
}

pub type SharedState<S> = Arc<RwLock<S>>;

/// A state that represents how nodes are interconnected in the network.
pub struct OverlayState {
    pub all_nodes: Vec<NodeId>,
    pub overlays: BTreeMap<usize, ViewOverlay>,
}

pub trait OverlayGetter {
    fn get_view(&self, index: usize) -> Option<ViewOverlay>;
    fn get_all_nodes(&self) -> Vec<NodeId>;
}

impl OverlayGetter for SharedState<OverlayState> {
    fn get_view(&self, index: usize) -> Option<ViewOverlay> {
        let overlay_state = self.read();
        overlay_state.overlays.get(&index).cloned()
    }

    fn get_all_nodes(&self) -> Vec<NodeId> {
        let overlay_state = self.read();
        overlay_state.all_nodes.clone()
    }
}

pub trait Node {
    type Settings;
    type State;

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
