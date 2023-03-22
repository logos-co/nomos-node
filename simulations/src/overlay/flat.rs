use rand::prelude::IteratorRandom;
use rand::Rng;
// std
// crates
// internal
use super::Overlay;
use crate::node::NodeId;
use crate::overlay::{Committee, Layout};

pub struct FlatOverlay;

impl Overlay for FlatOverlay {
    type Settings = ();

    fn new(_settings: Self::Settings) -> Self {
        Self
    }

    fn leaders<R: Rng>(
        &self,
        nodes: &[NodeId],
        size: usize,
        rng: &mut R,
    ) -> Box<dyn Iterator<Item = NodeId>> {
        let leaders = nodes.iter().copied().choose_multiple(rng, size).into_iter();
        Box::new(leaders)
    }

    fn layout<R: Rng>(&self, nodes: &[NodeId], _rng: &mut R) -> Layout {
        let committees =
            std::iter::once((0.into(), nodes.iter().copied().collect::<Committee>())).collect();
        let parent = std::iter::once((0.into(), 0.into())).collect();
        let children = std::iter::once((0.into(), vec![0.into()])).collect();
        Layout::new(committees, parent, children)
    }
}
