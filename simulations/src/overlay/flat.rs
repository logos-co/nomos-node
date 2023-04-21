// std
// crates
use rand::prelude::IteratorRandom;
use rand::Rng;
// internal
use super::Overlay;
use crate::node::NodeId;
use crate::overlay::{Committee, Layout};

pub struct FlatOverlay;
impl FlatOverlay {
    fn new() -> Self {
        Self
    }
}

impl Overlay for FlatOverlay {
    fn nodes(&self) -> Vec<NodeId> {
        (0..10).map(NodeId::from).collect()
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
        let committees = std::iter::once((
            0.into(),
            Committee {
                nodes: nodes.iter().copied().collect(),
            },
        ))
        .collect();
        let parent = std::iter::once((0.into(), 0.into())).collect();
        let children = std::iter::once((0.into(), vec![0.into()])).collect();
        let layers = std::iter::once((0.into(), vec![0.into()])).collect();
        Layout::new(committees, parent, children, layers)
    }
}
