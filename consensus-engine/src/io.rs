use crate::{Id, Payload, View};
use std::collections::HashSet;
use std::hash::Hash;

/// Possible events to which the consensus engine can react.
/// Timeout is here so that the engine can abstract over time
/// and remain a pure function.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Input {
    Block { block: Block },
    NewView { view: View },
    Timeout,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Block {
    pub id: Id,
    pub view: View,
    pub parent_qc: Qc,
}

impl Block {
    pub fn parent(&self) -> Id {
        self.parent_qc.block()
    }
}

/// Possible output events.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Output {
    /// block is "safe", meaning it's not for an old
    /// view and the qc is from the view previous to the
    /// one in which the block was issued.
    ///
    /// This can be used to filter out blocks for which
    /// a participant should actively vote for.
    SafeBlock {
        view: View,
        id: Id,
    },
    Send {
        to: HashSet<Id>,
        payload: Payload,
    },
    Broadcast {
        payload: Payload,
    },
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct StandardQc {
    pub view: View,
    pub id: Id,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct AggregateQc {
    pub qcs: Vec<View>,
    pub highest_qc: StandardQc,
    pub view: View,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Qc {
    Standard(StandardQc),
    Aggregated(AggregateQc),
}

impl Qc {
    /// The view of the block this qc is for.
    pub fn view(&self) -> View {
        match self {
            Qc::Standard(StandardQc { view, .. }) => *view,
            Qc::Aggregated(AggregateQc { highest_qc, .. }) => highest_qc.view,
        }
    }

    /// The id of the block this qc is for.
    /// This will be the parent of the block which will include this qc
    pub fn block(&self) -> Id {
        self.high_qc().id
    }

    pub fn high_qc(&self) -> StandardQc {
        match self {
            Qc::Standard(qc) => qc.clone(),
            Qc::Aggregated(AggregateQc { highest_qc, .. }) => highest_qc.clone(),
        }
    }
}
