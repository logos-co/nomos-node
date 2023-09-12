use consensus_engine::View;
#[cfg(feature = "polars")]
use polars::export::ahash::HashMap;
#[cfg(not(feature = "polars"))]
use std::collections::HashMap;
use std::time::Duration;

pub(crate) struct TimeoutHandler {
    pub timeout: Duration,
    pub per_view: HashMap<View, Duration>,
}

impl TimeoutHandler {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            per_view: Default::default(),
        }
    }

    pub fn step(&mut self, view: View, elapsed: Duration) -> bool {
        let timeout = self.per_view.entry(view).or_insert(self.timeout);
        *timeout = timeout.saturating_sub(elapsed);
        *timeout == Duration::ZERO
    }

    pub fn is_timeout(&self, view: View) -> bool {
        self.per_view
            .get(&view)
            .map(|t| t.is_zero())
            .unwrap_or(false)
    }

    pub fn prune_by_view(&mut self, view: View) {
        self.per_view.retain(|entry, _| entry > &view);
    }
}
