use consensus_engine::{Block, TimeoutQc};

// State transtitions that will be picked randomly
#[derive(Clone, Debug)]
pub enum Transition {
    Nop,
    ReceiveSafeBlock(Block),
    ReceiveUnsafeBlock(Block),
    ApproveBlock(Block),
    ApprovePastBlock(Block),
    LocalTimeout,
    ReceiveTimeoutQcForCurrentView(TimeoutQc),
    ReceiveTimeoutQcForOldView(TimeoutQc),
    //TODO: add more invalid transitions that must be rejected by consensus-engine
    //TODO: add more transitions
}
