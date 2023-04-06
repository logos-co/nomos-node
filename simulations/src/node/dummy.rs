// std
use std::collections::{BTreeMap, BTreeSet, VecDeque};
// crates
use crossbeam::channel::{Receiver, Sender};
use serde::{Deserialize, Serialize};
// internal
use crate::{
    network::{NetworkInterface, NetworkMessage},
    node::{Node, NodeId},
};

use super::{OverlayGetter, OverlayState, SharedState, ViewOverlay};

#[derive(Debug, Default, Serialize)]
pub struct DummyState {
    pub current_view: usize,
    pub message_count: usize,
    pub view_state: BTreeMap<usize, DummyViewState>,
}

#[derive(Debug, Default, Serialize)]
pub struct DummyViewState {
    proposal_received: bool,
    vote_received_count: usize,
    vote_sent: bool,
}

#[derive(Clone, Default, Deserialize)]
pub struct DummySettings {}

/// Helper intent to distinguish between votes ment for different roles in the tree
/// because dummy node does not compute QC.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum Intent {
    FromRootToLeader,
    FromInternalToRoot,
    #[default]
    FromLeafToInternal,
}

#[derive(Debug, Clone)]
pub struct Vote {
    pub view: usize,
    pub intent: Intent,
}

impl Vote {
    pub fn new(id: usize, intent: Intent) -> Self {
        Self { view: id, intent }
    }

    pub fn upgrade(&self, intent: Intent) -> Self {
        Self {
            view: self.view,
            intent,
        }
    }
}

impl From<usize> for Vote {
    fn from(id: usize) -> Self {
        Self {
            view: id,
            intent: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Block {
    pub view: usize,
}

impl Block {
    pub fn new(id: usize) -> Self {
        Self { view: id }
    }
}

impl From<usize> for Block {
    fn from(id: usize) -> Self {
        Self { view: id }
    }
}

#[derive(Debug, Clone)]
pub enum DummyMessage {
    Vote(Vote),
    Proposal(Block),
}

struct LocalView {
    pub next_view_leaders: Vec<NodeId>,
    pub current_roots: Option<BTreeSet<NodeId>>,
    pub children: Option<BTreeSet<NodeId>>,
    pub parents: Option<BTreeSet<NodeId>>,
    pub roles: Vec<DummyRole>,
}

impl LocalView {
    pub fn new(node_id: NodeId, view_id: usize, overlays: SharedState<OverlayState>) -> Self {
        let view = overlays
            .get_view(view_id)
            .expect("simulation generated enough views");
        let next_view = overlays
            .get_view(view_id + 1)
            .expect("simulation generated enough views");

        let parents = get_parent_nodes(node_id, &view);
        let children = get_child_nodes(node_id, &view);
        let roles = get_roles(node_id, &next_view, &parents, &children);
        // In tree layout CommitteeId(0) is always root committee.
        let current_roots = Some(
            view.layout
                .committees
                .get(&0.into())
                .expect("has CommitteeId(0)")
                .nodes
                .clone(),
        );

        Self {
            next_view_leaders: next_view.leaders,
            current_roots,
            children,
            parents,
            roles,
        }
    }
}

pub struct DummyNode {
    node_id: NodeId,
    state: DummyState,
    _settings: DummySettings,
    overlay_state: SharedState<OverlayState>,
    network_interface: DummyNetworkInterface,
    local_view: LocalView,

    // Node in current view might be a leader in the next view.
    // To prevent two states for different roles colliding, temp_leader_state
    // is used only for leaders. It is overridden when current_view is updated.
    temp_leader_state: DummyViewState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
        view_id: usize,
        overlay_state: SharedState<OverlayState>,
        network_interface: DummyNetworkInterface,
    ) -> Self {
        Self {
            node_id,
            state: Default::default(),
            _settings: Default::default(),
            overlay_state: overlay_state.clone(),
            network_interface,
            local_view: LocalView::new(node_id, view_id, overlay_state),
            temp_leader_state: Default::default(),
        }
    }

    pub fn send_message(&self, address: NodeId, message: DummyMessage) {
        self.network_interface.send_message(address, message);
    }

    fn broadcast(&self, addresses: &[NodeId], message: DummyMessage) {
        addresses
            .iter()
            .for_each(|address| self.send_message(*address, message.clone()))
    }

    fn update_view(&mut self, view: usize) {
        self.state.view_state.insert(
            view,
            DummyViewState {
                proposal_received: true,
                ..Default::default()
            },
        );
        self.temp_leader_state = Default::default();
        self.state.current_view = view;
        self.local_view = LocalView::new(self.id(), view, self.overlay_state.clone());
    }

    fn is_vote_sent(&self, view: usize) -> bool {
        self.state
            .view_state
            .get(&view)
            .expect("view state created")
            .vote_sent
    }

    fn set_vote_sent(&mut self, view: usize) {
        let view_state = self
            .state
            .view_state
            .get_mut(&view)
            .expect("view state created");
        view_state.vote_sent = true;
    }

    fn get_vote_count(&self, view: usize) -> usize {
        self.state
            .view_state
            .get(&view)
            .expect("view state created")
            .vote_received_count
    }

    fn increment_vote_count(&mut self, view: usize) {
        let view_state = self
            .state
            .view_state
            .get_mut(&view)
            .expect("view state created");
        view_state.vote_received_count += 1;
    }

    fn has_enough_votes(&self, count: usize, children: &Option<BTreeSet<NodeId>>) -> bool {
        let children = children.as_ref().expect("has children");
        // TODO: Get percentage from the node settings.
        let enough = children.len() as f32 * 0.66;

        count > enough as usize
    }

    // Assumptions:
    // - Leader gets vote from root nodes that are in previous overlay.
    // - Leader sends NewView message to all it's view nodes if it receives votes from all root
    // nodes.
    fn handle_leader(&mut self, message: &NetworkMessage<DummyMessage>) {
        if let DummyMessage::Vote(vote) = &message.payload {
            // Internal node can be a leader in the next view, check if the vote traversed the
            // whole tree.
            if vote.intent != Intent::FromRootToLeader || vote.view < self.current_view() {
                return;
            }

            self.temp_leader_state.vote_received_count += 1;
            if !self.temp_leader_state.vote_sent
                && self.has_enough_votes(
                    self.temp_leader_state.vote_received_count,
                    &self.local_view.current_roots,
                )
            {
                let new_view_id = self.current_view() + 1;
                self.broadcast(
                    &self.overlay_state.get_all_nodes(),
                    DummyMessage::Proposal(new_view_id.into()),
                );
                self.temp_leader_state.vote_sent = true;
                self.update_view(new_view_id);
            }
        }
    }

    fn handle_root(&mut self, message: &NetworkMessage<DummyMessage>) {
        match &message.payload {
            DummyMessage::Vote(vote) => {
                // Root node can be a leader in the next view, check if the vote traversed the
                // whole tree.
                if vote.intent != Intent::FromInternalToRoot || vote.view != self.current_view() {
                    return;
                }

                self.increment_vote_count(vote.view);
                if !self.is_vote_sent(vote.view)
                    && self
                        .has_enough_votes(self.get_vote_count(vote.view), &self.local_view.children)
                {
                    self.broadcast(
                        &self.local_view.next_view_leaders,
                        DummyMessage::Vote(vote.upgrade(Intent::FromRootToLeader)),
                    );
                    self.set_vote_sent(vote.view);
                }
            }
            DummyMessage::Proposal(block) => {
                if block.view > self.current_view() {
                    self.update_view(block.view);
                }
            }
        }
    }

    fn handle_internal(&mut self, message: &NetworkMessage<DummyMessage>) {
        match &message.payload {
            DummyMessage::Vote(vote) => {
                // Internal node can be a leader in the next view, check if the vote traversed the
                // whole tree.
                if vote.intent != Intent::FromLeafToInternal || vote.view != self.current_view() {
                    return;
                }

                self.increment_vote_count(vote.view);
                if !self.is_vote_sent(vote.view)
                    && self
                        .has_enough_votes(self.get_vote_count(vote.view), &self.local_view.children)
                {
                    let parents = self
                        .local_view
                        .parents
                        .as_ref()
                        .expect("internal has parents");
                    parents.iter().for_each(|node_id| {
                        self.send_message(
                            *node_id,
                            DummyMessage::Vote(vote.upgrade(Intent::FromInternalToRoot)),
                        )
                    });
                    self.set_vote_sent(vote.view);
                }
            }
            DummyMessage::Proposal(block) => {
                if block.view > self.current_view() {
                    self.update_view(block.view);
                }
            }
        }
    }

    fn handle_leaf(&mut self, message: &NetworkMessage<DummyMessage>) {
        if let DummyMessage::Proposal(block) = &message.payload {
            if block.view > self.current_view() {
                self.update_view(block.view);
            }
            if !self.is_vote_sent(block.view) {
                let parents = &self.local_view.parents.as_ref().expect("leaf has parents");
                parents.iter().for_each(|node_id| {
                    self.send_message(*node_id, DummyMessage::Vote(block.view.into()))
                });
                self.set_vote_sent(block.view);
            }
        }
    }

    fn handle_unknown(&mut self, message: &NetworkMessage<DummyMessage>) {
        if let DummyMessage::Proposal(block) = &message.payload {
            if block.view > self.current_view() {
                self.update_view(block.view);
            }
        }
    }

    // The view can change on any message, node needs to change it's possition and roles if the
    // view changes during the message processing.
    fn handle_message(&mut self, message: &NetworkMessage<DummyMessage>) {
        let current_view = self.current_view();
        let mut roles: VecDeque<_> = self.local_view.roles.iter().cloned().collect();

        while let Some(role) = roles.pop_front() {
            match role {
                DummyRole::Leader => {
                    self.handle_leader(message);

                    // If leader is a leaf too, just process new proposal and send vote.
                    // if current_view < self.current_view()
                    //     && self.local_view.roles.contains(&DummyRole::Leaf)
                    // {
                    //     roles.push_back(DummyRole::Leaf);
                    // }
                }
                DummyRole::Root => self.handle_root(message),
                DummyRole::Internal => self.handle_internal(message),
                DummyRole::Leaf => self.handle_leaf(message),
                DummyRole::Unknown => self.handle_unknown(message), // not in the current view overlay?
            }
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
        self.state.message_count += incoming_messages.len();

        incoming_messages
            .iter()
            .for_each(|message| self.handle_message(message));
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
        println!(
            "sending from {:?} to {:?}, message {:?}",
            message.from, message.to, message.payload
        );
        self.sender.send(message).unwrap();
    }

    fn receive_messages(&self) -> Vec<crate::network::NetworkMessage<Self::Payload>> {
        self.receiver.try_iter().collect()
    }
}

// fn get_peer_nodes(node_id: NodeId, view: &ViewOverlay) -> Option<BTreeSet<NodeId>> {
//     let committee_id = view.layout.committee(node_id);
//     let nodes = &view.layout.committee_nodes(committee_id).nodes;
//     match nodes.len() {
//         0 => None,
//         _ => Some(nodes.clone()),
//     }
// }

fn get_parent_nodes(node_id: NodeId, view: &ViewOverlay) -> Option<BTreeSet<NodeId>> {
    let committee_id = view.layout.committee(node_id)?;
    view.layout.parent_nodes(committee_id).map(|c| c.nodes)
}

fn get_child_nodes(node_id: NodeId, view: &ViewOverlay) -> Option<BTreeSet<NodeId>> {
    let committee_id = view.layout.committee(node_id)?;
    let child_nodes: BTreeSet<NodeId> = view
        .layout
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
    next_view: &ViewOverlay,
    parents: &Option<BTreeSet<NodeId>>,
    children: &Option<BTreeSet<NodeId>>,
) -> Vec<DummyRole> {
    let mut roles = Vec::new();
    if next_view.leaders.contains(&node_id) {
        roles.push(DummyRole::Leader);
    }

    match (parents, children) {
        (None, Some(_)) => roles.push(DummyRole::Root),
        (Some(_), Some(_)) => roles.push(DummyRole::Internal),
        (Some(_), None) => roles.push(DummyRole::Leaf),
        (None, None) => {
            //roles.push(DummyRole::Root);
            //roles.push(DummyRole::Leaf);
            roles.push(DummyRole::Unknown);
        }
    };
    roles
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet, HashMap},
        sync::{Arc, RwLock},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use crossbeam::channel;
    use rand::{
        rngs::{mock::StepRng, SmallRng},
        SeedableRng,
    };

    use crate::{
        network::{
            behaviour::NetworkBehaviour,
            regions::{Region, RegionsData},
            Network,
        },
        node::{
            dummy::{get_child_nodes, get_parent_nodes, get_roles, DummyRole},
            Node, NodeId, OverlayState, SharedState, ViewOverlay,
        },
        overlay::{
            tree::{TreeOverlay, TreeSettings},
            Overlay,
        },
    };

    use super::{DummyMessage, DummyNetworkInterface, DummyNode, Intent, Vote};

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
    ) -> HashMap<NodeId, DummyNode> {
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
                (
                    *node_id,
                    DummyNode::new(*node_id, 0, overlay_state.clone(), network_interface),
                )
            })
            .collect()
    }

    #[test]
    fn send_receive_tree_overlay_steprng() {
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

        let view = ViewOverlay {
            leaders: vec![0.into(), 1.into(), 2.into()],
            layout: overlay.layout(&node_ids, &mut rng),
        };
        let overlay_state = Arc::new(RwLock::new(OverlayState {
            all_nodes: node_ids.clone(),
            overlays: BTreeMap::from([
                (0, view.clone()),
                (1, view.clone()),
                (2, view.clone()),
                (3, view),
            ]),
        }));

        let mut nodes = init_dummy_nodes(&node_ids, &mut network, overlay_state);
        let initial_vote = Vote::new(1, Intent::FromRootToLeader);

        // Using any node as the sender for initial proposal to leader nodes.
        nodes[&0.into()].send_message(0.into(), DummyMessage::Vote(initial_vote.clone()));
        nodes[&0.into()].send_message(1.into(), DummyMessage::Vote(initial_vote.clone()));
        nodes[&0.into()].send_message(2.into(), DummyMessage::Vote(initial_vote));
        network.collect_messages();

        for (_, node) in nodes.iter() {
            assert_eq!(node.current_view(), 0);
        }

        // 1. Leaders receive vote and broadcast new Proposal(Block) to all nodes.
        network.dispatch_after(&mut rng, Duration::from_millis(100));
        nodes.iter_mut().for_each(|(_, node)| {
            node.step();
        });
        network.collect_messages();

        // 2. a) All nodes received proposal block.
        //    b) Leaf nodes send vote to internal nodes.
        network.dispatch_after(&mut rng, Duration::from_millis(100));
        nodes.iter_mut().for_each(|(_, node)| {
            node.step();
        });
        network.collect_messages();

        // All nodes should be updated to the proposed blocks view.
        for (_, node) in nodes.iter() {
            assert_eq!(node.current_view(), 1);
        }

        // Root and Internal haven't sent their votes yet.
        assert!(!nodes[&0.into()].state().view_state[&1].vote_sent); // Root
        assert!(!nodes[&1.into()].state().view_state[&1].vote_sent); // Internal
        assert!(!nodes[&2.into()].state().view_state[&1].vote_sent); // Internal

        // Leaves should have thier vote sent.
        assert!(nodes[&3.into()].state().view_state[&1].vote_sent); // Leaf
        assert!(nodes[&4.into()].state().view_state[&1].vote_sent); // Leaf
        assert!(nodes[&5.into()].state().view_state[&1].vote_sent); // Leaf
        assert!(nodes[&6.into()].state().view_state[&1].vote_sent); // Leaf

        // 3. Internal nodes send vote to root node.
        network.dispatch_after(&mut rng, Duration::from_millis(100));
        nodes.iter_mut().for_each(|(_, node)| {
            node.step();
        });
        network.collect_messages();

        // Root hasn't sent its votes yet.
        assert!(!nodes[&0.into()].state().view_state[&1].vote_sent); // Root

        // Internal and leaves should have thier vote sent.
        assert!(nodes[&1.into()].state().view_state[&1].vote_sent); // Internal
        assert!(nodes[&2.into()].state().view_state[&1].vote_sent); // Internal
        assert!(nodes[&3.into()].state().view_state[&1].vote_sent); // Leaf
        assert!(nodes[&4.into()].state().view_state[&1].vote_sent); // Leaf
        assert!(nodes[&5.into()].state().view_state[&1].vote_sent); // Leaf
        assert!(nodes[&6.into()].state().view_state[&1].vote_sent); // Leaf

        // 4. Root node send vote to next view leader nodes.
        network.dispatch_after(&mut rng, Duration::from_millis(100));
        nodes.iter_mut().for_each(|(_, node)| {
            node.step();
        });
        network.collect_messages();

        // Root has sent its votes.
        assert!(nodes[&0.into()].state().view_state[&1].vote_sent); // Root
        assert!(nodes[&1.into()].state().view_state[&1].vote_sent); // Internal
        assert!(nodes[&2.into()].state().view_state[&1].vote_sent); // Internal
        assert!(nodes[&3.into()].state().view_state[&1].vote_sent); // Leaf
        assert!(nodes[&4.into()].state().view_state[&1].vote_sent); // Leaf
        assert!(nodes[&5.into()].state().view_state[&1].vote_sent); // Leaf
        assert!(nodes[&6.into()].state().view_state[&1].vote_sent); // Leaf

        // 5. Leaders receive vote and broadcast new Proposal(Block) to all nodes.
        network.dispatch_after(&mut rng, Duration::from_millis(100));
        nodes.iter_mut().for_each(|(_, node)| {
            node.step();
        });
        network.collect_messages();

        // Leader sould have their view incremented.
        // Other nodes should still remain in previous view.
        assert_eq!(nodes[&0.into()].current_view(), 2); // Root, Leader
        assert_eq!(nodes[&1.into()].current_view(), 2); // Internal, Leader
        assert_eq!(nodes[&2.into()].current_view(), 2); // Internal, Leader
        assert_eq!(nodes[&3.into()].current_view(), 1); // Leaf
        assert_eq!(nodes[&4.into()].current_view(), 1); // Leaf
        assert_eq!(nodes[&5.into()].current_view(), 1); // Leaf
        assert_eq!(nodes[&6.into()].current_view(), 1); // Leaf

        // 6. a) All nodes received proposal block.
        //    b) Leaf nodes send vote to internal nodes.
        network.dispatch_after(&mut rng, Duration::from_millis(100));
        nodes.iter_mut().for_each(|(_, node)| {
            node.step();
        });
        network.collect_messages();

        // All nodes should be updated to the proposed blocks view.
        for (_, node) in nodes.iter() {
            assert_eq!(node.current_view(), 2); // new
        }

        // Root and Internal haven't sent their votes yet.
        assert!(!nodes[&0.into()].state().view_state[&2].vote_sent); // Root
        assert!(!nodes[&1.into()].state().view_state[&2].vote_sent); // Internal
        assert!(!nodes[&2.into()].state().view_state[&2].vote_sent); // Internal

        // Leaves should have thier vote sent.
        assert!(nodes[&3.into()].state().view_state[&2].vote_sent); // Leaf
        assert!(nodes[&4.into()].state().view_state[&2].vote_sent); // Leaf
        assert!(nodes[&5.into()].state().view_state[&2].vote_sent); // Leaf
        assert!(nodes[&6.into()].state().view_state[&2].vote_sent); // Leaf
    }

    #[test]
    fn send_receive_tree_overlay_smallrng() {
        // Nodes should progress to second view in 6 steps with random nodes in tree overlay if
        // network conditions are the same as in `send_receive_tree_overlay` test.
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("valid unix timestamp")
            .as_secs();
        println!("{timestamp}");
        let timestamp = 1680815658;
        let mut rng = SmallRng::seed_from_u64(timestamp);
        let overlay = TreeOverlay::new(TreeSettings {
            tree_type: Default::default(),
            depth: 3,
            committee_size: 1,
        });
        //let node_ids: Vec<NodeId> = overlay.nodes();
        let node_ids: Vec<NodeId> = (0..20).map(Into::into).collect();
        let mut network = init_network(&node_ids);

        let overlays: BTreeMap<usize, ViewOverlay> = (0..=3)
            .map(|view_id| {
                (
                    view_id,
                    ViewOverlay {
                        leaders: overlay.leaders(&node_ids, 3, &mut rng).collect(),
                        layout: overlay.layout(&node_ids, &mut rng),
                    },
                )
            })
            .collect();
        let overlay_state = Arc::new(RwLock::new(OverlayState {
            all_nodes: node_ids.clone(),
            overlays: overlays.clone(),
        }));

        let mut nodes = init_dummy_nodes(&node_ids, &mut network, overlay_state);
        let initial_vote = Vote::new(1, Intent::FromRootToLeader);

        // Using any node as the sender for initial proposal to leader nodes.
        overlays
            .get(&1)
            .unwrap()
            .leaders
            .iter()
            .for_each(|leader_id| {
                nodes[&0.into()].send_message(*leader_id, DummyMessage::Vote(initial_vote.clone()));
            });
        network.collect_messages();

        for (_, node) in nodes.iter() {
            assert_eq!(node.current_view(), 0);
        }

        for i in 0..7 {
            println!(">>>>>>>>>>>>>>>>>>>>>>> Step {i}");
            network.dispatch_after(&mut rng, Duration::from_millis(100));
            nodes.iter_mut().for_each(|(_, node)| {
                node.step();
            });
            network.collect_messages();
        }

        for (_, node) in nodes.iter() {
            println!("{:?}; state: {:?}", node.id(), node.state());
            //assert_eq!(node.current_view(), 2);
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
                vec![DummyRole::Leader, DummyRole::Leaf],
            ),
        ];
        let mut rng = StepRng::new(1, 0);
        let overlay = TreeOverlay::new(TreeSettings {
            tree_type: Default::default(),
            depth: 3,
            committee_size: 1,
        });
        let node_ids: Vec<NodeId> = overlay.nodes();
        let leaders = vec![6.into()];
        let layout = overlay.layout(&node_ids, &mut rng);
        let view = ViewOverlay { leaders, layout };

        for (node_id, expected_parents, expected_children, expected_roles) in test_cases {
            let node_id = node_id.into();
            let parents = get_parent_nodes(node_id, &view);
            let children = get_child_nodes(node_id, &view);
            let role = get_roles(node_id, &view, &parents, &children);
            assert_eq!(parents, expected_parents);
            assert_eq!(children, expected_children);
            assert_eq!(role, expected_roles);
        }
    }
}
