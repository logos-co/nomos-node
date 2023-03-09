// std

// crates
use futures::{Stream, StreamExt};
// internal
use crate::block::BlockId;
use crate::vote::Tally;

pub enum QuorumCertificate {
    Simple(SimpleQuorumCertificate),
    Aggregated(AggregatedQuorumCertificate),
}

impl QuorumCertificate {
    pub fn view(&self) -> u64 {
        match self {
            QuorumCertificate::Simple(qc) => qc.view,
            QuorumCertificate::Aggregated(qc) => qc.view,
        }
    }
}

pub struct SimpleQuorumCertificate {
    view: u64,
    block: BlockId,
}

pub struct AggregatedQuorumCertificate {
    view: u64,
    high_qh: Box<QuorumCertificate>,
}

pub struct Vote {
    block: BlockId,
    view: u64,
    voter: usize, // TODO: this should be some id, probably the node pk
    qc: Option<QuorumCertificate>,
}

impl Vote {
    pub fn valid_view(&self, view: u64) -> bool {
        self.view == view && self.qc.as_ref().map_or(true, |qc| qc.view() == view - 1)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CarnotTallyError {
    #[error("Received invalid vote")]
    InvalidVote,
    #[error("Did not receive enough votes")]
    InsufficientVotes,
}

#[derive(Clone)]
pub struct CarnotTallySettings {
    threshold: usize,
}

pub struct CarnotTally {
    settings: CarnotTallySettings,
}

#[async_trait::async_trait]
impl Tally for CarnotTally {
    type Vote = Vote;
    type Outcome = QuorumCertificate;
    type TallyError = CarnotTallyError;
    type Settings = CarnotTallySettings;

    fn new(settings: Self::Settings) -> Self {
        Self { settings }
    }

    async fn tally<S: Stream<Item = Self::Vote> + Unpin + Send>(
        &self,
        view: u64,
        mut vote_stream: S,
    ) -> Result<Self::Outcome, Self::TallyError> {
        let mut approved = 0usize;
        while let Some(vote) = vote_stream.next().await {
            if !vote.valid_view(view) {
                return Err(CarnotTallyError::InvalidVote);
            }
            approved += 1;
            if approved >= self.settings.threshold {
                return Ok(QuorumCertificate::Simple(SimpleQuorumCertificate {
                    view,
                    block: vote.block,
                }));
            }
        }
        Err(CarnotTallyError::InsufficientVotes)
    }
}
