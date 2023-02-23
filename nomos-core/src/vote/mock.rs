// std
// crates
use futures::{Stream, StreamExt};
// internal
use crate::vote::Tally;

pub enum MockVote {
    Yes,
    No,
}

pub enum QC {
    Approved(usize, usize),
    Denied(usize, usize),
}

pub struct Error(String);

#[derive(Clone)]
pub struct MockTallySettings {
    threshold: usize,
}

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
        mut vote_stream: S,
    ) -> Result<Self::Outcome, Self::TallyError> {
        let mut yes = 0;
        let mut no = 0;
        // TODO: use a timeout
        while let Some(vote) = vote_stream.next().await {
            match vote {
                MockVote::Yes => yes += 1,
                MockVote::No => no += 1,
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
