pub mod flat;
pub mod tree;

// std
use std::collections::{BTreeSet, HashMap};
// crates
use rand::Rng;
use serde::{Deserialize, Serialize};
// internal
use crate::node::{CommitteeId, NodeId};

use self::tree::TreeSettings;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Committee {
    pub nodes: BTreeSet<NodeId>,
}

impl Committee {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.len() == 0
    }
}

pub type Leaders = BTreeSet<NodeId>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Layout {
    pub committees: HashMap<CommitteeId, Committee>,
    pub from_committee: HashMap<NodeId, CommitteeId>,
    pub parent: HashMap<CommitteeId, CommitteeId>,
    pub children: HashMap<CommitteeId, Vec<CommitteeId>>,
    pub layers: HashMap<CommitteeId, Vec<CommitteeId>>,
}

impl Layout {
    pub fn new(
        committees: HashMap<CommitteeId, Committee>,
        parent: HashMap<CommitteeId, CommitteeId>,
        children: HashMap<CommitteeId, Vec<CommitteeId>>,
        layers: HashMap<CommitteeId, Vec<CommitteeId>>,
    ) -> Self {
        let from_committee = committees
            .iter()
            .flat_map(|(&committee_id, committee)| {
                committee
                    .nodes
                    .iter()
                    .map(move |&node_id| (node_id, committee_id))
            })
            .collect();
        Self {
            committees,
            from_committee,
            parent,
            children,
            layers,
        }
    }

    pub fn committee(&self, node_id: NodeId) -> Option<CommitteeId> {
        self.from_committee.get(&node_id).copied()
    }

    pub fn committee_nodes(&self, committee_id: CommitteeId) -> &Committee {
        &self.committees[&committee_id]
    }

    pub fn parent(&self, committee_id: CommitteeId) -> Option<CommitteeId> {
        self.parent.get(&committee_id).copied()
    }

    pub fn parent_nodes(&self, committee_id: CommitteeId) -> Option<Committee> {
        self.parent(committee_id)
            .map(|c| self.committees[&c].clone())
    }

    pub fn children(&self, committee_id: CommitteeId) -> Option<&Vec<CommitteeId>> {
        self.children.get(&committee_id)
    }

    pub fn children_nodes(&self, committee_id: CommitteeId) -> Vec<&Committee> {
        self.children(committee_id)
            .iter()
            .flat_map(|&committees| committees.iter().map(|c| &self.committees[c]))
            .collect()
    }

    pub fn node_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.from_committee.keys().copied()
    }
}

pub enum SimulationOverlay {
    Flat(flat::FlatOverlay),
    Tree(tree::TreeOverlay),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OverlaySettings {
    Flat,
    Tree(TreeSettings),
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self::Tree(Default::default())
    }
}

impl From<TreeSettings> for OverlaySettings {
    fn from(settings: TreeSettings) -> OverlaySettings {
        OverlaySettings::Tree(settings)
    }
}

impl TryInto<TreeSettings> for OverlaySettings {
    type Error = String;

    fn try_into(self) -> Result<TreeSettings, Self::Error> {
        if let Self::Tree(settings) = self {
            Ok(settings)
        } else {
            Err("unable to convert to tree settings".into())
        }
    }
}

impl Overlay for SimulationOverlay {
    fn nodes(&self) -> Vec<NodeId> {
        match self {
            SimulationOverlay::Flat(overlay) => overlay.nodes(),
            SimulationOverlay::Tree(overlay) => overlay.nodes(),
        }
    }

    fn leaders<R: Rng>(
        &self,
        nodes: &[NodeId],
        size: usize,
        rng: &mut R,
    ) -> Box<dyn Iterator<Item = NodeId>> {
        match self {
            SimulationOverlay::Flat(overlay) => overlay.leaders(nodes, size, rng),
            SimulationOverlay::Tree(overlay) => overlay.leaders(nodes, size, rng),
        }
    }

    fn layout<R: Rng>(&self, nodes: &[NodeId], rng: &mut R) -> Layout {
        match self {
            SimulationOverlay::Flat(overlay) => overlay.layout(nodes, rng),
            SimulationOverlay::Tree(overlay) => overlay.layout(nodes, rng),
        }
    }
}

pub trait Overlay {
    fn nodes(&self) -> Vec<NodeId>;
    fn leaders<R: Rng>(
        &self,
        nodes: &[NodeId],
        size: usize,
        rng: &mut R,
    ) -> Box<dyn Iterator<Item = NodeId>>;
    fn layout<R: Rng>(&self, nodes: &[NodeId], rng: &mut R) -> Layout;
}

// Takes a reference to the simulation_settings and returns a SimulationOverlay instance based
// on the overlay settings specified in simulation_settings.
pub fn create_overlay(overlay_settings: &OverlaySettings) -> SimulationOverlay {
    match &overlay_settings {
        OverlaySettings::Flat => SimulationOverlay::Flat(flat::FlatOverlay::new()),
        OverlaySettings::Tree(settings) => {
            SimulationOverlay::Tree(tree::TreeOverlay::new(settings.clone()))
        }
    }
}
