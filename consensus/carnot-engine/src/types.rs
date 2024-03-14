// std
use std::hash::Hash;
// crates
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

mod committee;
pub use committee::{Committee, CommitteeId};
mod node_id;
pub use node_id::NodeId;
mod view;
pub use view::View;

/// The way the consensus engine communicates with the rest of the system is by returning
/// actions to be performed.
/// Often, the actions are to send a message to a set of nodes.
/// This enum represents the different types of messages that can be sent from the perspective of consensus and
/// can't be directly used in the network as they lack things like cryptographic signatures.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Payload<Id> {
    /// Vote for a block in a view
    Vote(Vote<Id>),
    /// Signal that a local timeout has occurred
    Timeout(Timeout<Id>),
    /// Vote for moving to a new view
    NewView(NewView<Id>),
}

/// Returned
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Vote<Id> {
    pub view: View,
    pub block: Id,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Timeout<Id> {
    pub view: View,
    pub sender: NodeId,
    pub high_qc: StandardQc<Id>,
    pub timeout_qc: Option<TimeoutQc<Id>>,
}

// TODO: We are making "mandatory" to have received the timeout_qc before the new_view votes.
// We should consider to remove the TimoutQc from the NewView message and use a hash or id instead.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NewView<Id> {
    pub view: View,
    pub sender: NodeId,
    pub timeout_qc: TimeoutQc<Id>,
    pub high_qc: StandardQc<Id>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TimeoutQc<Id> {
    view: View,
    high_qc: StandardQc<Id>,
    sender: NodeId,
}

impl<Id> TimeoutQc<Id> {
    pub fn new(view: View, high_qc: StandardQc<Id>, sender: NodeId) -> Self {
        assert!(
            view >= high_qc.view,
            "timeout_qc.view:{} shouldn't be lower than timeout_qc.high_qc.view:{}",
            view,
            high_qc.view,
        );

        Self {
            view,
            high_qc,
            sender,
        }
    }

    pub fn view(&self) -> View {
        self.view
    }

    pub fn high_qc(&self) -> &StandardQc<Id> {
        &self.high_qc
    }

    pub fn sender(&self) -> NodeId {
        self.sender
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Block<Id> {
    pub id: Id,
    pub view: View,
    pub parent_qc: Qc<Id>,
    pub leader_proof: LeaderProof,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LeaderProof {
    LeaderId { leader_id: NodeId },
}

impl<Id: Copy> Block<Id> {
    pub fn parent(&self) -> Id {
        self.parent_qc.block()
    }

    pub fn genesis(id: Id) -> Self {
        Self {
            view: View(0),
            id,
            parent_qc: Qc::Standard(StandardQc::genesis(id)),
            leader_proof: LeaderProof::LeaderId {
                leader_id: NodeId::new([0; 32]),
            },
        }
    }
}

/// Possible output events.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Send<Id> {
    pub to: Committee,
    pub payload: Payload<Id>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StandardQc<Id> {
    pub view: View,
    pub id: Id,
}

impl<Id> StandardQc<Id> {
    pub fn genesis(id: Id) -> Self {
        Self { view: View(-1), id }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AggregateQc<Id> {
    pub high_qc: StandardQc<Id>,
    pub view: View,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Qc<Id> {
    Standard(StandardQc<Id>),
    Aggregated(AggregateQc<Id>),
}

impl<Id: Copy> Qc<Id> {
    /// The view in which this Qc was built.
    pub fn view(&self) -> View {
        match self {
            Qc::Standard(StandardQc { view, .. }) => *view,
            Qc::Aggregated(AggregateQc { view, .. }) => *view,
        }
    }

    /// The id of the block this qc is for.
    /// This will be the parent of the block which will include this qc
    pub fn block(&self) -> Id {
        match self {
            Qc::Standard(StandardQc { id, .. }) => *id,
            Qc::Aggregated(AggregateQc { high_qc, .. }) => high_qc.id,
        }
    }

    pub fn high_qc(&self) -> StandardQc<Id> {
        match self {
            Qc::Standard(qc) => qc.clone(),
            Qc::Aggregated(AggregateQc { high_qc, .. }) => high_qc.clone(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn standard_qc() {
        let standard_qc = StandardQc {
            view: View(10),
            id: 0,
        };
        let qc = Qc::Standard(standard_qc.clone());
        assert_eq!(qc.view(), View(10));
        assert_eq!(qc.block(), 0);
        assert_eq!(qc.high_qc(), standard_qc);
    }

    #[test]
    fn aggregated_qc() {
        let aggregated_qc = AggregateQc {
            view: View(20),
            high_qc: StandardQc {
                view: View(10),
                id: 0,
            },
        };
        let qc = Qc::Aggregated(aggregated_qc.clone());
        assert_eq!(qc.view(), View(20));
        assert_eq!(qc.block(), 0);
        assert_eq!(qc.high_qc(), aggregated_qc.high_qc);
    }

    #[test]
    fn new_timeout_qc() {
        let timeout_qc = TimeoutQc::new(
            View(2),
            StandardQc {
                view: View(1),
                id: 0,
            },
            NodeId::new([0; 32]),
        );
        assert_eq!(timeout_qc.view(), View(2));
        assert_eq!(timeout_qc.high_qc().view, View(1));
        assert_eq!(timeout_qc.high_qc().id, 0);
        assert_eq!(timeout_qc.sender(), NodeId::new([0; 32]));

        let timeout_qc = TimeoutQc::new(
            View(2),
            StandardQc {
                view: View(2),
                id: 0,
            },
            NodeId::new([0; 32]),
        );
        assert_eq!(timeout_qc.view(), View(2));
        assert_eq!(timeout_qc.high_qc().view, View(2));
        assert_eq!(timeout_qc.high_qc().id, 0);
        assert_eq!(timeout_qc.sender(), NodeId::new([0; 32]));
    }

    #[test]
    #[should_panic(
        expected = "timeout_qc.view:1 shouldn't be lower than timeout_qc.high_qc.view:2"
    )]
    fn new_timeout_qc_panic() {
        let _ = TimeoutQc::new(
            View(1),
            StandardQc {
                view: View(2),
                id: 0,
            },
            NodeId::new([0; 32]),
        );
    }
}
