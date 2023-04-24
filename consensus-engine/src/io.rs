use crate::{Id, View};
use std::any::Any;

/// Possible events to which the consensus engine can react.
/// Timeout is here so that the engine can abstract over time
/// and remain a pure function.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Input {
    Block { block: Block },
    NewView { view: View },
    Timeout,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Copy)]
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
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
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
        to: Id,
        payload: Box<dyn Any + Eq + PartialEq + Hash>,
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Copy)]
pub enum Qc {
    Standard {
        view: View,
        id: Id,
    },
    Aggregated {
        // FIXME: is this ever used?
        view: View,
        // This could be a field of type Qc but that would
        // make the type recursive and we don't want that.
        // It's guaranteed that a high_qc is always 'Standard',
        // so we collapse this to holding just the view and the id.
        high_qc_view: View,
        high_qc_id: Id,
    },
    
    
}

impl Qc {
    /// The view of the block this qc is for.
    pub fn view(&self) -> View {
        match self {
            Qc::Standard { view, .. } => *view,
            // FIXME: what about aggQc.view?
            Qc::Aggregated { high_qc_view, .. } => *high_qc_view,
        }
    }

    /// The id of the block this qc is for.
    /// This will be the parent of the block which will include this qc
    pub fn block(&self) -> Id {
        match self {
            Qc::Standard { id, .. } => *id,
            Qc::Aggregated { high_qc_id, .. } => *high_qc_id,
        }
    }
}
