use crate::SlotTick;
use cryptarchia_engine::{Epoch, Slot};
use futures::Stream;

#[async_trait::async_trait]
pub trait TimeBackend {
    type Settings;
    fn init(settings: Self::Settings) -> Self;
    async fn tick(self) -> impl Stream<Item = SlotTick>;
}
