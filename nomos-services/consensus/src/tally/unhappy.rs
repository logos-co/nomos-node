// std
use std::collections::HashSet;
// crates
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
// internal
use super::CarnotTallySettings;
use crate::network::messages::NewViewMsg;
use consensus_engine::{NewView, TimeoutQc};
use nomos_core::vote::Tally;

#[derive(thiserror::Error, Debug)]
pub enum NewViewTallyError {
    #[error("Did not receive enough votes")]
    InsufficientVotes,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewViewTally {
    settings: CarnotTallySettings,
}

#[async_trait::async_trait]
impl Tally for NewViewTally {
    type Vote = NewViewMsg;
    type Qc = ();
    type Subject = TimeoutQc;
    type Outcome = HashSet<NewView>;
    type TallyError = NewViewTallyError;
    type Settings = CarnotTallySettings;

    fn new(settings: Self::Settings) -> Self {
        Self { settings }
    }

    async fn tally<S: Stream<Item = Self::Vote> + Unpin + Send>(
        &self,
        timeout_qc: TimeoutQc,
        mut vote_stream: S,
    ) -> Result<(Self::Qc, Self::Outcome), Self::TallyError> {
        let mut seen = HashSet::new();
        let mut outcome = HashSet::new();

        // return early for leaf nodes
        if self.settings.threshold == 0 {
            return Ok(((), outcome));
        }

        while let Some(vote) = vote_stream.next().await {
            // check vote view is valid
            if !vote.vote.view != timeout_qc.view {
                continue;
            }

            // check for individual nodes votes
            if !self.settings.participating_nodes.contains(&vote.voter) {
                continue;
            }
            seen.insert(vote.voter);
            outcome.insert(vote.vote.clone());
            if seen.len() >= self.settings.threshold {
                return Ok(((), outcome));
            }
        }
        Err(NewViewTallyError::InsufficientVotes)
    }
}
