// std
use std::collections::HashSet;
use std::hash::Hash;
// crates
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub type View = i64;
pub type NodeId = [u8; 32];
pub type BlockId = [u8; 32];
pub type Committee = HashSet<NodeId>;

/// The way the consensus engine communicates with the rest of the system is by returning
/// actions to be performed.
/// Often, the actions are to send a message to a set of nodes.
/// This enum represents the different types of messages that can be sent from the perspective of consensus and
/// can't be directly used in the network as they lack things like cryptographic signatures.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Payload {
    /// Vote for a block in a view
    Vote(Vote),
    /// Signal that a local timeout has occurred
    Timeout(Timeout),
    /// Vote for moving to a new view
    NewView(NewView),
}

/// Returned
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Vote {
    pub view: View,
    pub block: BlockId,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Timeout {
    pub view: View,
    pub sender: NodeId,
    pub high_qc: StandardQc,
    pub timeout_qc: Option<TimeoutQc>,
}

// TODO: We are making "mandatory" to have received the timeout_qc before the new_view votes.
// We should consider to remove the TimoutQc from the NewView message and use a hash or id instead.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct NewView {
    pub view: View,
    pub sender: NodeId,
    pub timeout_qc: TimeoutQc,
    pub high_qc: StandardQc,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TimeoutQc {
    pub view: View,
    pub high_qc: StandardQc,
    pub sender: NodeId,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Block {
    #[cfg_attr(feature = "serde", serde(skip))]
    pub id: BlockId,
    pub view: View,
    pub parent_qc: Qc,
}

impl Block {
    pub fn parent(&self) -> BlockId {
        self.parent_qc.block()
    }
}

/// Possible output events.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Send {
    pub to: HashSet<NodeId>,
    pub payload: Payload,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct StandardQc {
    pub view: View,
    pub id: BlockId,
}

impl StandardQc {
    pub fn genesis() -> Self {
        Self {
            view: -1,
            id: [0; 32],
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct AggregateQc {
    pub high_qc: StandardQc,
    pub view: View,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Qc {
    Standard(StandardQc),
    Aggregated(AggregateQc),
}

impl Qc {
    /// The view in which this Qc was built.
    pub fn view(&self) -> View {
        match self {
            Qc::Standard(StandardQc { view, .. }) => *view,
            Qc::Aggregated(AggregateQc { view, .. }) => *view,
        }
    }

    /// The view of the block this qc is for.
    pub fn parent_view(&self) -> View {
        match self {
            Qc::Standard(StandardQc { view, .. }) => *view,
            Qc::Aggregated(AggregateQc { view, .. }) => *view,
        }
    }

    /// The id of the block this qc is for.
    /// This will be the parent of the block which will include this qc
    pub fn block(&self) -> BlockId {
        match self {
            Qc::Standard(StandardQc { id, .. }) => *id,
            Qc::Aggregated(AggregateQc { high_qc, .. }) => high_qc.id,
        }
    }

    pub fn high_qc(&self) -> StandardQc {
        match self {
            Qc::Standard(qc) => qc.clone(),
            Qc::Aggregated(AggregateQc { high_qc, .. }) => high_qc.clone(),
        }
    }
}

pub trait Overlay: Clone {
    fn new(nodes: Vec<NodeId>) -> Self;
    fn root_committee(&self) -> Committee;
    fn rebuild(&mut self, timeout_qc: TimeoutQc);
    fn is_member_of_child_committee(&self, parent: NodeId, child: NodeId) -> bool;
    fn is_member_of_root_committee(&self, id: NodeId) -> bool;
    fn is_member_of_leaf_committee(&self, id: NodeId) -> bool;
    fn is_child_of_root_committee(&self, id: NodeId) -> bool;
    fn parent_committee(&self, id: NodeId) -> Committee;
    fn child_committees(&self, id: NodeId) -> Vec<Committee>;
    fn leaf_committees(&self, id: NodeId) -> Vec<Committee>;
    fn node_committee(&self, id: NodeId) -> Committee;
    fn leader(&self, view: View) -> NodeId;
    fn super_majority_threshold(&self, id: NodeId) -> usize;
    fn leader_super_majority_threshold(&self, id: NodeId) -> usize;
}
