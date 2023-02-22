pub mod mock;

use futures::Stream;

#[async_trait::async_trait]
pub trait Tally {
    type Vote;
    type Outcome;
    type TallyError;
    type Settings;
    fn new(settings: Self::Settings) -> Self;
    async fn tally<S: Stream<Item = Self::Vote> + Unpin + Send>(
        &self,
        vote_stream: S,
    ) -> Result<Self::Outcome, Self::TallyError>;
}
