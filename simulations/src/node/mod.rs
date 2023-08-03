pub mod carnot;
pub mod dummy;

#[cfg(test)]
pub mod dummy_streaming;

// std
use consensus_engine::{Committee, View};
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
use crate::overlay::{Layout, OverlaySettings, SimulationOverlay};

pub use consensus_engine::NodeId;

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

impl From<OverlaySettings> for ViewOverlay {
    fn from(value: OverlaySettings) -> Self {
        match value {
            OverlaySettings::Flat => {
                todo!()
            }
            OverlaySettings::Tree(_) => {
                todo!()
            }
        }
    }
}

pub type SharedState<S> = Arc<RwLock<S>>;

/// A state that represents how nodes are interconnected in the network.
pub struct OverlayState {
    pub all_nodes: Vec<NodeId>,
    pub overlay: SimulationOverlay,
    pub overlays: BTreeMap<View, ViewOverlay>,
}

pub trait OverlayGetter {
    fn get_view(&self, index: View) -> Option<ViewOverlay>;
    fn get_all_nodes(&self) -> Vec<NodeId>;
}

impl OverlayGetter for SharedState<OverlayState> {
    fn get_view(&self, index: View) -> Option<ViewOverlay> {
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
    fn current_view(&self) -> View;
    fn state(&self) -> &Self::State;
    fn step(&mut self, elapsed: Duration);
}

#[cfg(test)]
impl Node for usize {
    type Settings = ();
    type State = Self;

    fn id(&self) -> NodeId {
        NodeId::from_index(*self)
    }

    fn current_view(&self) -> View {
        View::new(*self as i64)
    }

    fn state(&self) -> &Self::State {
        self
    }

    fn step(&mut self, _: Duration) {
        use std::ops::AddAssign;
        self.add_assign(1);
    }
}

pub trait NodeIdExt {
    fn index(&self) -> usize;

    fn from_index(idx: usize) -> Self;
}

impl NodeIdExt for NodeId {
    fn index(&self) -> usize {
        const SIZE: usize = core::mem::size_of::<usize>();
        let mut bytes = [0u8; SIZE];
        let src: [u8; 32] = (*self).into();
        bytes.copy_from_slice(&src[..SIZE]);
        usize::from_be_bytes(bytes)
    }

    fn from_index(idx: usize) -> Self {
        let mut bytes = [0u8; 32];
        bytes[..core::mem::size_of::<usize>()].copy_from_slice(&idx.to_be_bytes());
        NodeId::new(bytes)
    }
}

pub trait ResearchNodeIdExt: NodeIdExt {
    fn parent_index(&self) -> usize;
}

impl ResearchNodeIdExt for NodeId {
    fn parent_index(&self) -> usize {
        const SIZE: usize = core::mem::size_of::<usize>();
        let x: [u8; 32] = (*self).into();
        let mut buf = [0; SIZE];
        buf.copy_from_slice(&x[32 - SIZE..]);
        usize::from_be_bytes(buf)
    }
}

/// Reference https://github.com/logos-co/nomos-specs/blob/Carnot-Simulation/carnot/carnot_simulation_psuedocode.py
pub trait CommitteeExt {
    /// Fill a binary tree of committees given the number of nodes.
    fn fill(&mut self, num_nodes: usize, idx: usize, parent_idx: Option<usize>);

    /// Returns the leftmost committee
    fn leftmost_committee(&self) -> Option<&NodeId>;

    /// Returns the rightmost committee
    fn rightmost_committee(&self) -> Option<&NodeId>;

    /// Returns the depth of the binary tree.
    ///
    /// level start from 0, not 1
    fn depth(&self) -> usize;

    /// Simulates the message passing from a committee to its parent committee.
    ///
    /// Returns the number of levels the message passed through.
    fn simulate_message_passing(&self, lvl: usize, latency: Duration) -> usize;
}

impl CommitteeExt for Committee {
    fn fill(&mut self, num_nodes: usize, idx: usize, parent_idx: Option<usize>) {
        const USIZE: usize = core::mem::size_of::<usize>();
        if num_nodes == 0 {
            return;
        }

        let id = if let Some(parent_idx) = parent_idx {
            let mut buf = [0; 32];
            // use first size_of::<usize> bits to store level
            buf[..USIZE].copy_from_slice(idx.to_be_bytes().as_ref());
            // use last 8 bits to store parent id
            buf[32 - USIZE..].copy_from_slice(parent_idx.to_be_bytes().as_ref());
            NodeId::new(buf)
        } else {
            let mut buf = [0; 32];
            // use first 8 bits to store level
            buf[..8].copy_from_slice(idx.to_be_bytes().as_ref());
            NodeId::new(buf)
        };

        self.insert(id);
        self.fill(num_nodes - 1, idx + 1, Some(idx));
    }

    fn leftmost_committee(&self) -> Option<&NodeId> {
        if self.is_empty() {
            return None;
        }

        if self.len() == 1 {
            return self.iter().next();
        }

        let depth = self.depth();
        let min_index_at_depth = 2_usize.pow(depth as u32) - 1;

        self.iter()
            .map(|id| id.index())
            .find(|idx| *idx >= min_index_at_depth)
            .map(|idx| self.iter().find(|id| id.index() == idx).unwrap())
    }

    fn rightmost_committee(&self) -> Option<&NodeId> {
        let idx = self.iter().map(|id| id.index()).max();
        idx.map(|idx| self.iter().find(|id| id.index() == idx).unwrap())
    }

    fn depth(&self) -> usize {
        let max_index = match self.iter().map(|id| id.index()).max() {
            Some(max_node) => max_node,
            None => return 0,
        };

        (max_index as f64).log2().floor() as usize
    }

    fn simulate_message_passing(&self, lvl: usize, latency: Duration) -> usize {
        if lvl == 0 {
            return 0;
        }
        let depth = self.depth();
        if lvl > depth {
            return depth - 1;
        }

        let min_index_at_lvl = 2_usize.pow(lvl as u32) - 1;
        std::thread::sleep(latency * min_index_at_lvl as u32);
        // for depth 5, we only need pass 4 times, so minus 1
        min_index_at_lvl - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fill_and_depth() {
        let mut committee = Committee::new();
        committee.fill(5, 0, None);
        assert_eq!(committee.depth(), 2);
    }

    #[test]
    fn test_leftmost_committee() {
        let mut committee = Committee::new();
        committee.fill(5, 0, None);
        assert_eq!(committee.leftmost_committee().unwrap().index(), 3);
    }

    #[test]
    fn test_rightmost_committee() {
        let mut committee = Committee::new();
        committee.fill(5, 0, None);
        assert_eq!(committee.rightmost_committee().unwrap().index(), 4);
    }

    #[test]
    fn test_fill_and_depth_for_one_node() {
        let mut committee = Committee::new();
        committee.fill(1, 0, None);
        assert_eq!(committee.depth(), 0);
    }

    #[test]
    fn test_fill_and_depth_for_zero_nodes() {
        let mut committee = Committee::new();
        committee.fill(0, 0, None);
        assert_eq!(committee.depth(), 0);
    }

    #[test]
    fn test_leftmost_committee_for_one_node() {
        let mut committee = Committee::new();
        committee.fill(1, 0, None);
        assert_eq!(committee.leftmost_committee().unwrap().index(), 0);
    }

    #[test]
    fn test_rightmost_committee_for_one_node() {
        let mut committee = Committee::new();
        committee.fill(1, 0, None);
        assert_eq!(committee.rightmost_committee().unwrap().index(), 0);
    }

    #[test]
    fn test_leftmost_committee_for_zero_nodes() {
        let committee = Committee::new();
        assert!(committee.leftmost_committee().is_none());
    }

    #[test]
    fn test_rightmost_committee_for_zero_nodes() {
        let committee = Committee::new();
        assert!(committee.rightmost_committee().is_none());
    }

    #[test]
    fn test_simulate_msg_passing() {
        let mut committee = Committee::new();
        committee.fill(15, 0, None); // lvl 3
        assert_eq!(committee.depth(), 3);
        assert_eq!(
            committee.simulate_message_passing(2, Duration::from_millis(1)),
            2
        );
    }
}
