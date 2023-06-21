use consensus_engine::View;
use std::collections::{HashMap, HashSet};

pub(crate) struct Tally<T: core::hash::Hash + Eq> {
    cache: HashMap<View, HashSet<T>>,
    threshold: usize,
}

impl<T: core::hash::Hash + Eq> Default for Tally<T> {
    fn default() -> Self {
        Self::new(0)
    }
}

impl<T: core::hash::Hash + Eq> Tally<T> {
    pub fn new(threshold: usize) -> Self {
        Self {
            cache: Default::default(),
            threshold,
        }
    }

    pub fn tally(&mut self, view: View, message: T) -> Option<HashSet<T>> {
        self.tally_by(view, message, self.threshold)
    }

    pub fn tally_by(&mut self, view: View, message: T, threshold: usize) -> Option<HashSet<T>> {
        let entries = self.cache.entry(view).or_default();
        entries.insert(message);
        let entries = entries.len();
        if entries >= threshold {
            Some(self.cache.remove(&view).unwrap())
        } else {
            None
        }
    }
}
