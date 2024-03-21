use std::collections::HashSet;

use crate::fuzz::{Block, NewView, TimeoutQc};

// State transtitions that will be picked randomly
#[derive(Clone, Debug)]
pub enum Transition {
    Nop,
    ReceiveSafeBlock(Block),
    ReceiveUnsafeBlock(Block),
    ApproveBlock(Block),
    ApprovePastBlock(Block),
    LocalTimeout,
    ReceiveTimeoutQcForRecentView(TimeoutQc),
    ReceiveTimeoutQcForOldView(TimeoutQc),
    ApproveNewViewWithLatestTimeoutQc(TimeoutQc, HashSet<NewView>),
    //TODO: add more corner transitions
}
