// std
// crates
use rand::prelude::IteratorRandom;
use rand::Rng;
// internal
use super::Overlay;
use crate::node::carnot::{CarnotNode, CarnotRole};
use crate::node::NodeId;
use crate::overlay::{Committee, Layout};

pub struct FlatOverlay;

impl Overlay<CarnotNode> for FlatOverlay {
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
        let committees = std::iter::once((
            0,
            Committee {
                nodes: nodes.iter().copied().collect(),
                role: CarnotRole::Leader,
            },
        ))
        .collect();
        let parent = std::iter::once((0, 0)).collect();
        let children = std::iter::once((0, vec![0])).collect();
        let layers = std::iter::once((0, vec![0])).collect();
        Layout::new(committees, parent, children, layers)
    }
}
