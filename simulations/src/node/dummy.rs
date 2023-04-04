// std
use std::collections::BTreeSet;
// crates
use crossbeam::channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
// internal
use crate::{
    network::{NetworkInterface, NetworkMessage},
    node::{Node, NodeId},
    overlay::Layout,
};

use super::{OverlayState, SharedState};

#[derive(Debug, Default, Serialize)]
pub struct DummyState {
    pub current_view: usize,
    pub message_count: usize,
    pub view_state: DummyViewState,
}

#[derive(Debug, Default, Serialize)]
pub struct DummyViewState {
    pub child_message_count: usize,
    pub parent_message_count: usize,
    pub peer_message_count: usize,
}

#[derive(Clone, Default, Deserialize)]
pub struct DummySettings {}

#[derive(Clone)]
pub enum DummyMessage {
    Vote(usize),
    Proposal(usize),
    NewView(usize),
    RootProposal(usize),
    LeaderProposal(usize),
}

pub struct DummyNode {
    node_id: NodeId,
    state: DummyState,
    _settings: DummySettings,
    overlay_state: SharedState<OverlayState>,
    network_interface: DummyNetworkInterface,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DummyRole {
    Leader,
    Root,
    Internal,
    Leaf,
    Unknown,
}

impl DummyNode {
    pub fn new(
        node_id: NodeId,
        overlay_state: SharedState<OverlayState>,
        network_interface: DummyNetworkInterface,
    ) -> Self {
        Self {
            node_id,
            state: Default::default(),
            _settings: Default::default(),
            overlay_state,
            network_interface,
        }
    }

    pub fn send_message(&self, address: NodeId, message: DummyMessage) {
        self.network_interface.send_message(address, message);
    }

    fn on_proposal(
        &mut self,
        proposal: usize,
        roles: &[DummyRole],
        peers: &Option<BTreeSet<NodeId>>,
        parents: &Option<BTreeSet<NodeId>>,
        _children: &Option<BTreeSet<NodeId>>,
    ) {
        // Root - Check with peers.
        if roles.contains(&DummyRole::Root) {
            if let Some(peers) = peers {
                for peer in peers {
                    self.send_message(*peer, DummyMessage::RootProposal(proposal));
                }
            }
        }
        // Intermediary - Send proposal to parent.
        if roles.contains(&DummyRole::Internal) {
            if let Some(parents) = parents {
                for parent in parents {
                    self.send_message(*parent, DummyMessage::Proposal(proposal));
                }
            }
        }
        // Leaf - ignore.
    }

    fn on_root_proposal(
        &mut self,
        proposal: usize,
        roles: &[DummyRole],
        leaders: &[NodeId],
        peers: &Option<BTreeSet<NodeId>>,
    ) {
        // Root - Check with peers.
        if roles.contains(&DummyRole::Root) {
            let peer_msg_count = self.state.view_state.peer_message_count + 1;
            if peer_msg_count == peers.as_ref().unwrap().len() {
                for leader in leaders {
                    self.send_message(*leader, DummyMessage::Proposal(proposal));
                }
            }
            self.state.view_state.peer_message_count = peer_msg_count;
        }
    }

    fn on_leader_proposal(
        &mut self,
        proposal: usize,
        roles: &[DummyRole],
        all_nodes: &mut impl Iterator<Item = NodeId>,
    ) {
        // TODO: Check proposal?
        // Leader - increment the view if valid.
        if roles.contains(&DummyRole::Leader) {
            let root_msg_count = self.state.view_state.child_message_count + 1;
            if root_msg_count > 0 {
                self.state.current_view += 1;
                for node in all_nodes {
                    self.send_message(node, DummyMessage::NewView(self.state.current_view));
                }
            }
        }
    }

    fn on_vote(
        &mut self,
        vote: usize,
        roles: &[DummyRole],
        parents: &Option<BTreeSet<NodeId>>,
        children: &Option<BTreeSet<NodeId>>,
    ) {
        // Leader - ignore.
        // Root and intermediary - send vote to child.
        if roles.contains(&DummyRole::Root) || roles.contains(&DummyRole::Internal) {
            if let Some(children) = children {
                for child in children {
                    self.send_message(*child, DummyMessage::Vote(vote));
                }
            }
        }

        // Leaf - send vote to parents.
        // TODO: Should wait some time before sending vote.
        if roles.contains(&DummyRole::Leaf) {
            if let Some(parents) = parents {
                for parent in parents {
                    self.send_message(*parent, DummyMessage::Proposal(0));
                }
            }
        }
    }

    fn on_new_view(&mut self, view: usize, roles: &[DummyRole]) {
        if roles.len() > 1 || !roles.contains(&DummyRole::Leader) {
            self.state.current_view = view;
        }
    }
}

impl Node for DummyNode {
    type Settings = DummySettings;
    type State = DummyState;

    fn id(&self) -> NodeId {
        self.node_id
    }

    fn current_view(&self) -> usize {
        self.state.current_view
    }

    fn state(&self) -> &DummyState {
        &self.state
    }

    fn step(&mut self) {
        let incoming_messages = self.network_interface.receive_messages();
        let current_view = self.current_view();
        let (leaders, layout) = {
            let state = self.overlay_state.read().unwrap();
            println!(
                "nodeid: {:?}, current view {:?}",
                self.node_id, current_view
            );
            let view = &state.views[&current_view];
            (view.leaders.clone(), view.layout.clone())
        };
        let peers = get_peer_nodes(self.node_id, &layout);
        let parents = get_parent_nodes(self.node_id, &layout);
        let children = get_child_nodes(self.node_id, &layout);
        let roles = get_roles(self.node_id, &leaders, &parents, &children);

        for message in incoming_messages {
            self.state.message_count += 1;
            match message.payload {
                DummyMessage::NewView(v) => self.on_new_view(v, &roles),
                DummyMessage::Vote(v) => self.on_vote(v, &roles, &parents, &children),
                DummyMessage::Proposal(p) => {
                    self.on_proposal(p, &roles, &peers, &parents, &children)
                }
                DummyMessage::RootProposal(p) => {
                    self.on_root_proposal(p, &roles, &leaders, &peers);
                }
                DummyMessage::LeaderProposal(p) => {
                    let mut all_nodes = layout.node_ids();
                    self.on_leader_proposal(p, &roles, &mut all_nodes);
                }
            }
        }
    }
}

pub struct DummyNetworkInterface {
    id: NodeId,
    sender: Sender<NetworkMessage<DummyMessage>>,
    receiver: Receiver<NetworkMessage<DummyMessage>>,
}

impl DummyNetworkInterface {
    pub fn new(
        id: NodeId,
        sender: Sender<NetworkMessage<DummyMessage>>,
        receiver: Receiver<NetworkMessage<DummyMessage>>,
    ) -> Self {
        Self {
            id,
            sender,
            receiver,
        }
    }
}

impl NetworkInterface for DummyNetworkInterface {
    type Payload = DummyMessage;

    fn send_message(&self, address: NodeId, message: Self::Payload) {
        let message = NetworkMessage::new(self.id, address, message);
        self.sender.send(message).unwrap();
    }

    fn receive_messages(&self) -> Vec<crate::network::NetworkMessage<Self::Payload>> {
        self.receiver.try_iter().collect()
    }
}

fn get_peer_nodes(node_id: NodeId, layout: &Layout) -> Option<BTreeSet<NodeId>> {
    let committee_id = layout.committee(node_id);
    let nodes = &layout.committee_nodes(committee_id).nodes;
    match nodes.len() {
        0 => None,
        _ => Some(nodes.clone()),
    }
}

fn get_parent_nodes(node_id: NodeId, layout: &Layout) -> Option<BTreeSet<NodeId>> {
    let committee_id = layout.committee(node_id);
    layout.parent_nodes(committee_id).map(|c| c.nodes)
}

fn get_child_nodes(node_id: NodeId, layout: &Layout) -> Option<BTreeSet<NodeId>> {
    let committee_id = layout.committee(node_id);
    let child_nodes: BTreeSet<NodeId> = layout
        .children_nodes(committee_id)
        .iter()
        .flat_map(|c| c.nodes.clone())
        .collect();
    match child_nodes.len() {
        0 => None,
        _ => Some(child_nodes),
    }
}

fn get_roles(
    node_id: NodeId,
    leaders: &[NodeId],
    parents: &Option<BTreeSet<NodeId>>,
    children: &Option<BTreeSet<NodeId>>,
) -> Vec<DummyRole> {
    let mut roles = Vec::new();
    if leaders.contains(&node_id) {
        roles.push(DummyRole::Leader);
    }

    match (parents, children) {
        (None, Some(_)) => roles.push(DummyRole::Root),
        (Some(_), Some(_)) => roles.push(DummyRole::Internal),
        (Some(_), None) => roles.push(DummyRole::Leaf),
        (None, None) => {
            roles.push(DummyRole::Root);
            roles.push(DummyRole::Leaf);
        }
    };
    roles
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet, HashMap},
        sync::{Arc, RwLock},
        time::Duration,
    };

    use crossbeam::channel;
    use rand::rngs::mock::StepRng;

    use crate::{
        network::{
            behaviour::NetworkBehaviour,
            regions::{Region, RegionsData},
            Network,
        },
        node::{
            dummy::{get_child_nodes, get_parent_nodes, get_roles, DummyRole},
            Node, NodeId, OverlayState, SharedState, View,
        },
        overlay::{
            tree::{TreeOverlay, TreeSettings},
            Overlay,
        },
    };

    use super::{DummyMessage, DummyNetworkInterface, DummyNode};

    fn init_network(node_ids: &[NodeId]) -> Network<DummyMessage> {
        let regions = HashMap::from([(Region::Europe, node_ids.to_vec())]);
        let behaviour = HashMap::from([(
            (Region::Europe, Region::Europe),
            NetworkBehaviour::new(Duration::from_millis(100), 0.0),
        )]);
        let regions_data = RegionsData::new(regions, behaviour);
        Network::new(regions_data)
    }

    fn init_dummy_nodes(
        node_ids: &[NodeId],
        network: &mut Network<DummyMessage>,
        overlay_state: SharedState<OverlayState>,
    ) -> Vec<DummyNode> {
        node_ids
            .iter()
            .map(|node_id| {
                let (node_message_sender, node_message_receiver) = channel::unbounded();
                let network_message_receiver = network.connect(*node_id, node_message_receiver);
                let network_interface = DummyNetworkInterface::new(
                    *node_id,
                    node_message_sender,
                    network_message_receiver,
                );
                DummyNode::new(*node_id, overlay_state.clone(), network_interface)
            })
            .collect()
    }

    #[test]
    fn send_receive_tree_overlay() {
        let mut rng = StepRng::new(1, 0);
        //       0
        //   1       2
        // 3   4   5   6
        let overlay = TreeOverlay::new(TreeSettings {
            tree_type: Default::default(),
            depth: 3,
            committee_size: 1,
        });
        let node_ids: Vec<NodeId> = overlay.nodes();
        let mut network = init_network(&node_ids);

        let view = View {
            // leaders: overlay.leaders(&node_ids, 3, &mut rng).collect(),
            leaders: vec![0.into(), 1.into(), 2.into()],
            layout: overlay.layout(&node_ids, &mut rng),
        };
        let overlay_state = Arc::new(RwLock::new(OverlayState {
            views: BTreeMap::from([(0, view.clone())]),
        }));

        let mut nodes = init_dummy_nodes(&node_ids, &mut network, overlay_state);
        for node in nodes.iter() {
            if view.leaders.contains(&node.id()) {
                for n in nodes.iter() {
                    node.send_message(n.id(), DummyMessage::LeaderProposal(1));
                }
            }
        }
        network.collect_messages();

        for _ in 0..3 {
            network.dispatch_after(&mut rng, Duration::from_millis(100));
            nodes.iter_mut().for_each(|node| {
                node.step();
            });
            network.collect_messages();
        }
    }

    #[test]
    fn get_related_nodes() {
        //       0
        //   1       2
        // 3   4   5   6
        let test_cases = vec![
            (
                0,
                None,
                Some(BTreeSet::from([1.into(), 2.into()])),
                vec![DummyRole::Root],
            ),
            (
                1,
                Some(BTreeSet::from([0.into()])),
                Some(BTreeSet::from([3.into(), 4.into()])),
                vec![DummyRole::Internal],
            ),
            (
                2,
                Some(BTreeSet::from([0.into()])),
                Some(BTreeSet::from([5.into(), 6.into()])),
                vec![DummyRole::Internal],
            ),
            (
                3,
                Some(BTreeSet::from([1.into()])),
                None,
                vec![DummyRole::Leaf],
            ),
            (
                4,
                Some(BTreeSet::from([1.into()])),
                None,
                vec![DummyRole::Leaf],
            ),
            (
                5,
                Some(BTreeSet::from([2.into()])),
                None,
                vec![DummyRole::Leaf],
            ),
            (
                6,
                Some(BTreeSet::from([2.into()])),
                None,
                vec![DummyRole::Leaf],
            ),
        ];
        let mut rng = StepRng::new(1, 0);
        let overlay = TreeOverlay::new(TreeSettings {
            tree_type: Default::default(),
            depth: 3,
            committee_size: 1,
        });
        let node_ids: Vec<NodeId> = overlay.nodes();
        let leaders = vec![];
        let layout = overlay.layout(&node_ids, &mut rng);

        for (node_id, expected_parents, expected_children, expected_roles) in test_cases {
            let node_id = node_id.into();
            let parents = get_parent_nodes(node_id, &layout);
            let children = get_child_nodes(node_id, &layout);
            let role = get_roles(node_id, &leaders, &parents, &children);
            assert_eq!(parents, expected_parents);
            assert_eq!(children, expected_children);
            assert_eq!(role, expected_roles);
        }
    }
}
