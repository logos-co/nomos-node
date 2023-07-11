use consensus_engine::NodeId;
// std
// crates
use rand::prelude::IteratorRandom;
use rand::Rng;
// internal
use super::Overlay;
use crate::node::NodeIdExt;
use crate::overlay::{Committee, Layout};

pub struct FlatOverlay;
impl FlatOverlay {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FlatOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl Overlay for FlatOverlay {
    fn nodes(&self) -> Vec<NodeId> {
        (0..10).map(NodeId::from_index).collect()
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
