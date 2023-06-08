#![allow(dead_code)]

mod event_builder;
mod messages;

use std::{collections::HashMap, time::Duration};

use crate::network::{InMemoryNetworkInterface, NetworkInterface, NetworkMessage};
use crate::util::parse_idx;

// std
// crates
use self::messages::CarnotMessage;
use crate::node::carnot::event_builder::{CarnotTx, Event};
use consensus_engine::{
    Block, BlockId, Carnot, Committee, Overlay, Payload, Qc, StandardQc, TimeoutQc, View, Vote,
};
use nomos_consensus::network::messages::ProposalChunkMsg;
use nomos_consensus::{
    network::messages::{NewViewMsg, TimeoutMsg, VoteMsg},
    Output,
};
use serde::{Deserialize, Serialize};

// internal
use super::{Node, NodeId};

#[derive(Serialize)]
pub struct CarnotState {
    current_view: View,
    highest_voted_view: View,
    local_high_qc: StandardQc,
    #[serde(serialize_with = "serialize_blocks")]
    safe_blocks: HashMap<BlockId, Block>,
    last_view_timeout_qc: Option<TimeoutQc>,
    latest_committed_block: Block,
    latest_committed_view: View,
    root_committe: Committee,
    parent_committe: Committee,
    child_committees: Vec<Committee>,
    committed_blocks: Vec<BlockId>,
}

/// Have to implement this manually because of the `serde_json` will panic if the key of map
/// is not a string.
fn serialize_blocks<S>(blocks: &HashMap<BlockId, Block>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut ser = serializer.serialize_map(Some(blocks.len()))?;
    for (k, v) in blocks {
        ser.serialize_entry(&format!("{k:?}"), v)?;
    }
    ser.end()
}

impl<O: Overlay> From<&Carnot<O>> for CarnotState {
    fn from(value: &Carnot<O>) -> Self {
        let current_view = value.current_view();
        Self {
            current_view,
            local_high_qc: value.high_qc(),
            parent_committe: value.parent_committee(),
            root_committe: value.root_committee(),
            child_committees: value.child_committees(),
            latest_committed_block: value.latest_committed_block(),
            latest_committed_view: value.latest_committed_view(),
            safe_blocks: value
                .blocks_in_view(current_view)
                .into_iter()
                .map(|b| (b.id, b))
                .collect(),
            last_view_timeout_qc: value.last_view_timeout_qc(),
            committed_blocks: value.committed_blocks(),
            highest_voted_view: Default::default(),
        }
    }
}

#[derive(Clone, Default, Deserialize)]
pub struct CarnotSettings {
    nodes: Vec<consensus_engine::NodeId>,
    seed: u64,
    timeout: Duration,
}

impl CarnotSettings {
    pub fn new(nodes: Vec<consensus_engine::NodeId>, seed: u64, timeout: Duration) -> Self {
        Self {
            nodes,
            seed,
            timeout,
        }
    }
}

#[allow(dead_code)] // TODO: remove when handling settings
pub struct CarnotNode<O: Overlay> {
    id: consensus_engine::NodeId,
    state: CarnotState,
    settings: CarnotSettings,
    network_interface: InMemoryNetworkInterface<CarnotMessage>,
    event_builder: event_builder::EventBuilder,
    engine: Carnot<O>,
}

impl<O: Overlay> CarnotNode<O> {
    pub fn new(
        id: consensus_engine::NodeId,
        settings: CarnotSettings,
        network_interface: InMemoryNetworkInterface<CarnotMessage>,
    ) -> Self {
        let overlay = O::new(settings.nodes.clone());
        let genesis = nomos_consensus::Block::new(0, Block::genesis().parent_qc, [].into_iter());
        let engine = Carnot::from_genesis(id, genesis.header().clone(), overlay);
        let state = CarnotState::from(&engine);

        Self {
            id,
            state,
            settings,
            network_interface,
            event_builder: event_builder::EventBuilder::new(id, genesis),
            engine,
        }
    }

    pub(crate) fn send_message(&self, message: NetworkMessage<CarnotMessage>) {
        self.network_interface
            .send_message(self.id, message.payload);
    }

    fn handle_output(&self, output: Output<CarnotTx>) {
        match output {
            Output::Send(consensus_engine::Send {
                to,
                payload: Payload::Vote(vote),
            }) => {
                for node in to {
                    self.network_interface.send_message(
                        node,
                        CarnotMessage::Vote(VoteMsg {
                            voter: self.id,
                            vote: vote.clone(),
                            qc: Some(Qc::Standard(StandardQc {
                                view: vote.view,
                                id: vote.block,
                            })),
                        }),
                    );
                }
            }
            Output::Send(consensus_engine::Send {
                to,
                payload: Payload::NewView(new_view),
            }) => {
                for node in to {
                    self.network_interface.send_message(
                        node,
                        CarnotMessage::NewView(NewViewMsg {
                            voter: node,
                            vote: new_view.clone(),
                        }),
                    );
                }
            }
            Output::Send(consensus_engine::Send {
                to,
                payload: Payload::Timeout(timeout),
            }) => {
                for node in to {
                    self.network_interface.send_message(
                        node,
                        CarnotMessage::Timeout(TimeoutMsg {
                            voter: node,
                            vote: timeout.clone(),
                        }),
                    );
                }
            }
            Output::BroadcastTimeoutQc { .. } => {
                unimplemented!()
            }
            Output::BroadcastProposal { proposal } => {
                for node in &self.settings.nodes {
                    self.network_interface.send_message(
                        *node,
                        CarnotMessage::Proposal(ProposalChunkMsg {
                            chunk: proposal.as_bytes().to_vec().into(),
                            proposal: proposal.header().id,
                            view: proposal.header().view,
                        }),
                    )
                }
            }
        }
    }
}

impl<O: Overlay> Node for CarnotNode<O> {
    type Settings = CarnotSettings;
    type State = CarnotState;

    fn id(&self) -> NodeId {
        self.id
    }

    fn current_view(&self) -> usize {
        self.event_builder.current_view as usize
    }

    fn state(&self) -> &CarnotState {
        &self.state
    }

    fn step(&mut self) {
        let events = self.event_builder.step(
            self.network_interface
                .receive_messages()
                .into_iter()
                .map(|m| m.payload)
                .collect(),
            &self.engine,
        );

        for event in events {
            let mut output: Vec<Output<CarnotTx>> = vec![];
            match event {
                Event::Proposal { block } => {
                    let current_view = self.engine.current_view();
                    tracing::info!(
                        node=parse_idx(&self.id),
                        leader=parse_idx(&self.engine.leader(current_view)),
                        last_committed_view=self.engine.latest_committed_view(),
                        current_view = current_view,
                        block_view = block.header().view,
                        block = ?block.header().id,
                        parent_block=?block.header().parent(),
                        "receive block proposal",
                    );
                    match self.engine.receive_block(consensus_engine::Block {
                        id: block.header().id,
                        view: block.header().view,
                        parent_qc: block.header().parent_qc.clone(),
                    }) {
                        Ok(new) => self.engine = new,
                        Err(_) => {
                            tracing::error!(node = parse_idx(&self.id), current_view = self.engine.current_view(), block_view = block.header().view, block = ?block.header().id, "receive block proposal, but is invalid");
                        }
                    }

                    if self.engine.overlay().is_member_of_leaf_committee(self.id) {
                        output.push(nomos_consensus::Output::Send(consensus_engine::Send {
                            to: self.engine.parent_committee(),
                            payload: Payload::Vote(Vote {
                                view: self.engine.current_view(),
                                block: block.header().id,
                            }),
                        }))
                    }
                }
                // This branch means we already get enough votes for this block
                // So we can just call approve_block
                Event::Approve { block, .. } => {
                    tracing::info!(
                        node = parse_idx(&self.id),
                        leader=parse_idx(&self.engine.leader(block.view)),
                        current_view = self.engine.current_view(),
                        block_view = block.view,
                        block = ?block.id,
                        parent_block=?block.parent(),
                        "receive approve message"
                    );

                    // TODO: Remove
                    if block.view <= self.engine.highest_voted_view() {
                        tracing::error!("receive duplicated proposals");
                        continue;
                    }
                    let (new, out) = self.engine.approve_block(block);
                    tracing::info!(vote=?out, node=parse_idx(&self.id));
                    output = vec![Output::Send(out)];
                    self.engine = new;
                }
                Event::ProposeBlock { qc } => {
                    output = vec![Output::BroadcastProposal {
                        proposal: nomos_consensus::Block::new(qc.view() + 1, qc, [].into_iter()),
                    }]
                }
                // This branch means we already get enough new view msgs for this qc
                // So we can just call approve_new_view
                Event::NewView {
                    timeout_qc: _,
                    new_views: _,
                } => {
                    // let (new, out) = self.engine.approve_new_view(timeout_qc, new_views);
                    // output = Some(out);
                    // self.engine = new;
                    // let next_view = timeout_qc.view + 2;
                    // if self.engine.is_leader_for_view(next_view) {
                    //     self.gather_new_views(&[self.id].into_iter().collect(), timeout_qc);
                    // }
                    tracing::error!("unimplemented new view branch");
                    unimplemented!()
                }
                Event::TimeoutQc { timeout_qc } => {
                    self.engine = self.engine.receive_timeout_qc(timeout_qc);
                }
                Event::RootTimeout { timeouts } => {
                    println!("root timeouts: {timeouts:?}");
                }
                Event::LocalTimeout => {
                    tracing::error!("unimplemented local timeout branch");
                    unreachable!("local timeout will never be constructed")
                }
                Event::None => {
                    tracing::error!("unimplemented none branch");
                    unreachable!("none event will never be constructed")
                }
            }

            for output_event in output {
                self.handle_output(output_event);
            }
        }

        // update state
        self.state = CarnotState::from(&self.engine);
    }
}
