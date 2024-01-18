pub mod mock;
use futures::{Future, Stream};

pub trait Tally {
    type Vote;
    type Qc;
    type Outcome;
    type Subject;
    type TallyError;
    type Settings: Clone;
    fn new(settings: Self::Settings) -> Self;
    fn tally<S: Stream<Item = Self::Vote> + Unpin + Send>(
        &self,
        subject: Self::Subject,
        vote_stream: S,
    ) -> impl Future<Output = Result<(Self::Qc, Self::Outcome), Self::TallyError>> + Send;
}
