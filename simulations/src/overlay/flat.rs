use rand::prelude::IteratorRandom;
use rand::Rng;
// std
// crates
// internal
use super::Overlay;
use crate::node::NodeId;

pub struct FlatOverlay;

impl Overlay for FlatOverlay {
    fn leaders<R: Rng>(
        &self,
        nodes: &[NodeId],
        size: usize,
        rng: &mut R,
    ) -> Box<dyn Iterator<Item = NodeId>> {
        let leaders = nodes.iter().copied().choose_multiple(rng, size).into_iter();
        Box::new(leaders)
    }

    fn layout<R: Rng>(
        &self,
        nodes: &[NodeId],
        _rng: &mut R,
    ) -> Box<dyn Iterator<Item = Vec<NodeId>>> {
        Box::new(std::iter::once(nodes.to_vec()))
    }
}
