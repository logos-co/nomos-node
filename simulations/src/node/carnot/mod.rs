mod event_builder;
mod messages;

use std::collections::HashMap;

use crate::network::{InMemoryNetworkInterface, NetworkInterface, NetworkMessage};

// std
// crates
use self::{event_builder::EventBuilderSettings, messages::CarnotMessage, overlay::CarnotOverlay};
use consensus_engine::{View, StandardQc, BlockId, Block, TimeoutQc, Payload, Vote, Carnot, Overlay};
use nomos_consensus::Event;
use serde::{Deserialize, Serialize};

// internal
use super::{Node, NodeId};

#[derive(Default, Serialize)]
pub struct CarnotState {
    current_view: View,
    highest_voted_view: View,
    local_high_qc: StandardQc,
    safe_blocks: HashMap<BlockId, Block>,
    last_view_timeout_qc: Option<TimeoutQc>,
}

#[derive(Clone, Default, Deserialize)]
pub struct CarnotSettings {
    nodes: Vec<NodeId>,
}

#[allow(dead_code)] // TODO: remove when handling settings
pub struct CarnotNode<O: Overlay> {
    id: NodeId,
    state: CarnotState,
    settings: CarnotSettings,
    network_interface: InMemoryNetworkInterface<CarnotMessage>,
    event_builder: event_builder::EventBuilder,
    engine: Carnot<O>,
}

impl<O: Overlay> CarnotNode<O> {
    pub fn new(id: NodeId, settings: CarnotSettings) -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded();
        let genesis = consensus_engine::Block {
            id: [0; 32],
            view: 0,
            parent_qc: Qc::Standard(StandardQc::genesis()),
        };
        let overlay = O::new(settings.nodes);
        Self {
            id,
            state: Default::default(),
            settings,
            network_interface: InMemoryNetworkInterface::new(id, sender, receiver),
            event_builder: Default::default(),
            engine: Carnot::from_genesis(id, genesis, overlay),
        }
    }

    pub fn send_message(&self, message: NetworkMessage<CarnotMessage>) {
        self.network_interface.send_message(self.id, message);
    }

    pub fn approve_block(&self, block: Block) -> consensus_engine::Send {
        assert!(
            self.state.safe_blocks.contains_key(&block.id),
            "{:?} not in {:?}",
            block,
            self.state.safe_blocks
        );
        assert!(
            self.state.highest_voted_view < block.view,
            "can't vote for a block in the past"
        );

        // update state
        self.state.highest_voted_view = block.view;

        let to = if self.overlay.is_member_of_root_committee(self.id) {
            [self.overlay.leader(block.view + 1)]
                .into_iter()
                .collect()
        } else {
            self.overlay.parent_committee(self.id)
        };

        consensus_engine::Send {
            to,
            payload: Payload::Vote(Vote {
                block: block.id,
                view: block.view,
            }),
        }
    }

    fn is_leader_for_view(&self, view: View) -> bool {
        self.overlay.leader(view) == self.id
    }
}

impl<O: Overlay> Node for CarnotNode<O> {
    type Settings = CarnotSettings;
    type State = CarnotState;

    fn id(&self) -> NodeId {
        self.id
    }

    fn current_view(&self) -> usize {
        self.event_builder.current_view
    }

    fn state(&self) -> &CarnotState {
        &self.state
    }

    fn step(&mut self) {
        let events = self.event_builder.step(
            self.network_interface
                .receive_messages()
                .into_iter()
                .map(|m| m.payload),
        );

        for event in events {
            match event {
                Event::Proposal { block, stream } => {
                    println!("receive proposal {:?}", block.header().id);
                    self.engine.receive_block(block);
                },
                // This branch means we already get enough votes for this block
                // So we can just call approve_block
                Event::Approve { qc, block, votes } => {
                    self.engine.approve_block(block);
                },
                // This branch means we already get enough new view msgs for this qc
                // So we can just call approve_new_view 
                Event::NewView { timeout_qc, new_views } => {
                    self.engine.approve_new_view(timeout_qc, new_views);
                },
                Event::TimeoutQc { timeout_qc } => {
                    self.engine.receive_timeout_qc(timeout_qc);
                },
                Event::RootTimeout { timeouts } => {
                    println!("root timeouts: {:?}", timeouts);
                },
                Event::ProposeBlock { .. } => unreachable!("propose block will never be constructed"),
                Event::LocalTimeout => unreachable!("local timeout will never be constructed"),
                Event::None => unreachable!("none event will never be constructed"),
            }
        }
    }
}
