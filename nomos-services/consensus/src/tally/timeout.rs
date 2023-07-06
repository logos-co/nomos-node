// std
use std::{collections::HashSet, convert::Infallible};
// crates
use futures::{Stream, StreamExt};
// internal
use super::CarnotTallySettings;
use crate::network::messages::TimeoutMsg;
use consensus_engine::{Timeout, View};
use nomos_core::vote::Tally;

#[derive(Clone, Debug)]
pub struct TimeoutTally {
    settings: CarnotTallySettings,
}

#[async_trait::async_trait]
impl Tally for TimeoutTally {
    type Vote = TimeoutMsg;
    type Qc = ();
    type Subject = View;
    type Outcome = HashSet<Timeout>;
    type TallyError = Infallible;
    type Settings = CarnotTallySettings;

    fn new(settings: Self::Settings) -> Self {
        Self { settings }
    }

    async fn tally<S: Stream<Item = Self::Vote> + Unpin + Send>(
        &self,
        view: View,
        mut vote_stream: S,
    ) -> Result<(Self::Qc, Self::Outcome), Self::TallyError> {
        let mut seen = HashSet::new();
        let mut outcome = HashSet::new();
        while let Some(vote) = vote_stream.next().await {
            // check timeout view is valid
            if vote.vote.view != view {
                continue;
            }

            // check for individual nodes votes
            if !self
                .settings
                .participating_nodes
                .contains(&(vote.voter.into()))
            {
                continue;
            }

            seen.insert(vote.voter);
            outcome.insert(vote.vote.clone());
            if seen.len() >= self.settings.threshold {
                return Ok(((), outcome));
            }
        }
        unreachable!()
    }
}
