use crate::node::carnot::messages::CarnotMessage;
use consensus_engine::View;
use polars::export::ahash::HashMap;

pub(crate) struct MessageCache {
    cache: HashMap<View, Vec<CarnotMessage>>,
}

impl MessageCache {
    pub fn new() -> Self {
        Self {
            cache: Default::default(),
        }
    }

    pub fn update<I: IntoIterator<Item = CarnotMessage>>(&mut self, messages: I) {
        for message in messages {
            let entry = self.cache.entry(message.view()).or_default();
            entry.push(message);
        }
    }

    pub fn prune(&mut self, view: View) {
        self.cache.retain(|v, _| v > &view);
    }

    pub fn retrieve(&mut self, view: View) -> Vec<CarnotMessage> {
        self.cache.remove(&view).unwrap_or_default()
    }
}
