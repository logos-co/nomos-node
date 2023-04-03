use crate::{Id, View};

/// Possible events to which the consensus engine can react.
/// Timeout is here so that the engine can abstract over time
/// and remain a pure function.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Input {
    Proposal { view: View, id: Id, parent_qc: Qc },
    NewView { view: View },
    Timeout,
}

/// Possible output events.

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Output {
    /// Proposal is "safe", meaning it's not for an old
    /// view and the qc is from the view previous to the
    /// one in which the proposal was issued.
    ///
    /// This can be used to filter out proposals for which
    /// a participant should actively vote for.
    SafeProposal {
        view: View,
        id: Id,
    },
    Committed {
        view: View,
        id: Id,
    },
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
