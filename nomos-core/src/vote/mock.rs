// std
// crates
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
// internal
use crate::vote::Tally;

#[derive(Serialize, Deserialize)]
pub enum MockVote {
    Yes { view: u64 },
    No { view: u64 },
}

impl MockVote {
    pub fn view(&self) -> u64 {
        match self {
            MockVote::Yes { view } => *view,
            MockVote::No { view } => *view,
        }
    }
}

pub enum QC {
    Approved(usize, usize),
    Denied(usize, usize),
}

pub struct Error(String);

#[derive(Clone, Debug)]
pub struct MockTallySettings {
    pub threshold: usize,
}

#[derive(Debug)]
pub struct MockTally {
    threshold: usize,
}

#[async_trait::async_trait]
impl Tally for MockTally {
    type Vote = MockVote;
    type Outcome = QC;
    type TallyError = Error;
    type Settings = MockTallySettings;

    fn new(settings: Self::Settings) -> Self {
        let Self::Settings { threshold } = settings;
        Self { threshold }
    }

    async fn tally<S: Stream<Item = Self::Vote> + Unpin + Send>(
        &self,
        view: u64,
        mut vote_stream: S,
    ) -> Result<Self::Outcome, Self::TallyError> {
        let mut yes = 0;
        let mut no = 0;
        // TODO: use a timeout
        while let Some(vote) = vote_stream.next().await {
            if vote.view() != view {
                return Err(Error("Invalid vote".into()));
            }
            match vote {
                MockVote::Yes { .. } => yes += 1,
                MockVote::No { .. } => no += 1,
            }
        }
        if yes > self.threshold {
            Ok(QC::Approved(yes, no))
        } else if no > self.threshold {
            Ok(QC::Denied(yes, no))
        } else {
            Err(Error("Not enough votes".into()))
        }
    }
}
