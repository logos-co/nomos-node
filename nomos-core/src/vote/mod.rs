pub mod mock;

use consensus_engine::View;
use futures::Stream;

#[async_trait::async_trait]
pub trait Tally {
    type Vote;
    type Qc;
    type Outcome;
    type TallyError;
    type Settings: Clone;
    fn new(settings: Self::Settings) -> Self;
    async fn tally<S: Stream<Item = Self::Vote> + Unpin + Send>(
        &self,
        view: View,
        vote_stream: S,
    ) -> Result<(Self::Qc, Self::Outcome), Self::TallyError>;
}
