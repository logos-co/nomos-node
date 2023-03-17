// std
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;
use std::time::Duration;
// crates
use rand::prelude::SmallRng;
use rand::{Rng, SeedableRng};
// internal
use crate::network::Network;
use crate::node::{Node, NodeId, StepTime};
use crate::overlay::{Committee, Layout};

pub type RootCommitteeReceiverSolver = fn(&mut SmallRng, NodeId, &[NodeId], &Network) -> StepTime;

pub type ParentCommitteeReceiverSolver =
    fn(&mut SmallRng, NodeId, &Committee, &Network) -> StepTime;

pub type ChildCommitteeReceiverSolver =
    fn(&mut SmallRng, NodeId, &[&Committee], &Network) -> StepTime;

fn leader_receive_proposal(
    rng: &mut SmallRng,
    node: NodeId,
    leader_nodes: &[NodeId],
    network: &Network,
) -> StepTime {
    assert!(!leader_nodes.is_empty());
    leader_nodes
        .iter()
        .filter_map(|&sender| network.send_message_cost(rng, sender, node))
        .max()
        .unwrap()
}

fn receive_proposal(
    rng: &mut SmallRng,
    node: NodeId,
    committee: &Committee,
    network: &Network,
) -> StepTime {
    assert!(!committee.is_empty());
    committee
        .nodes
        .iter()
        .filter_map(|&sender| network.send_message_cost(rng, sender, node))
        .max()
        .unwrap()
}

fn receive_commit(
    rng: &mut SmallRng,
    node: NodeId,
    committees: &[&Committee],
    network: &Network,
) -> StepTime {
    assert!(!committees.is_empty());
    committees
        .iter()
        .filter_map(|committee| {
            committee
                .nodes
                .iter()
                .filter_map(|&sender| network.send_message_cost(rng, sender, node))
                .max()
        })
        .max()
        .unwrap()
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum CarnotStep {
    RootReceiveProposal,
    ReceiveProposal,
    ValidateProposal,
    LeaderReceiveVote,
    ReceiveVote,
    ValidateVote,
    Ignore,
}

impl core::str::FromStr for CarnotStep {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "receiveproposal" => Ok(Self::ReceiveProposal),
            "validateproposal" => Ok(Self::ValidateProposal),
            "receivevote" => Ok(Self::ReceiveVote),
            "validatevote" => Ok(Self::ValidateVote),
            _ => Err(format!("Unknown step: {s}")),
        }
    }
}

#[derive(Clone)]
pub enum CarnotStepSolver {
    Plain(StepTime),
    ParentCommitteeReceiverSolver(ParentCommitteeReceiverSolver),
    ChildCommitteeReceiverSolver(ChildCommitteeReceiverSolver),
    LeaderToCommitteeReceiverSolver(ChildCommitteeReceiverSolver),
    RootCommitteeReceiverSolver(RootCommitteeReceiverSolver),
}

pub struct CarnotNodeSettings {
    pub steps_costs: HashMap<CarnotStep, CarnotStepSolver>,
    pub network: Network,
    pub layout: Layout,
    pub leaders: Vec<NodeId>,
}

#[derive(Clone)]
pub struct CarnotNode {
    id: NodeId,
    rng: SmallRng,
    settings: Rc<CarnotNodeSettings>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CarnotRole {
    Leader,
    Root,
    Intermediate,
    Leaf,
}

pub const CARNOT_STEPS_COSTS: &[(CarnotStep, CarnotStepSolver)] = &[
    (
        CarnotStep::RootReceiveProposal,
        CarnotStepSolver::RootCommitteeReceiverSolver(leader_receive_proposal),
    ),
    (
        CarnotStep::ReceiveProposal,
        CarnotStepSolver::ParentCommitteeReceiverSolver(receive_proposal),
    ),
    (
        CarnotStep::ValidateProposal,
        CarnotStepSolver::Plain(StepTime::from_secs(1)),
    ),
    (
        CarnotStep::LeaderReceiveVote,
        CarnotStepSolver::LeaderToCommitteeReceiverSolver(receive_commit),
    ),
    (
        CarnotStep::ReceiveVote,
        CarnotStepSolver::ChildCommitteeReceiverSolver(receive_commit),
    ),
    (
        CarnotStep::ValidateVote,
        CarnotStepSolver::Plain(StepTime::from_secs(1)),
    ),
];

pub const CARNOT_LEADER_STEPS: &[CarnotStep] =
    &[CarnotStep::LeaderReceiveVote, CarnotStep::ValidateVote];

pub const CARNOT_ROOT_STEPS: &[CarnotStep] = &[
    CarnotStep::RootReceiveProposal,
    CarnotStep::ValidateProposal,
    CarnotStep::ReceiveVote,
    CarnotStep::ValidateVote,
];

pub const CARNOT_INTERMEDIATE_STEPS: &[CarnotStep] = &[
    CarnotStep::ReceiveProposal,
    CarnotStep::ValidateProposal,
    CarnotStep::ReceiveVote,
    CarnotStep::ValidateVote,
];

pub const CARNOT_LEAF_STEPS: &[CarnotStep] =
    &[CarnotStep::ReceiveProposal, CarnotStep::ValidateProposal];

pub const CARNOT_UNKNOWN_MESSAGE_RECEIVED_STEPS: &[CarnotStep] = &[CarnotStep::Ignore];

impl Node for CarnotNode {
    type Settings = Rc<CarnotNodeSettings>;

    fn new<R: Rng>(rng: &mut R, id: NodeId, settings: Self::Settings) -> Self {
        let rng = SmallRng::from_rng(rng).unwrap();
        Self { id, rng, settings }
    }

    fn id(&self) -> usize {
        self.id
    }

    fn run_steps(&mut self, steps: &[CarnotStep]) -> StepTime {
        use CarnotStepSolver::*;

        steps
            .iter()
            .map(|step| match self.settings.steps_costs.get(step) {
                Some(Plain(t)) => *t,
                Some(ParentCommitteeReceiverSolver(solver)) => {
                    match self
                        .settings
                        .layout
                        .parent_nodes(self.settings.layout.committee(self.id))
                    {
                        Some(parent) => {
                            solver(&mut self.rng, self.id, &parent, &self.settings.network)
                        }
                        None => Duration::ZERO,
                    }
                }
                Some(ChildCommitteeReceiverSolver(solver)) => solver(
                    &mut self.rng,
                    self.id,
                    &self
                        .settings
                        .layout
                        .children_nodes(self.settings.layout.committee(self.id)),
                    &self.settings.network,
                ),
                Some(RootCommitteeReceiverSolver(solver)) => solver(
                    &mut self.rng,
                    self.id,
                    &self.settings.leaders,
                    &self.settings.network,
                ),
                Some(LeaderToCommitteeReceiverSolver(solver)) => {
                    let committees: Vec<&Committee> =
                        self.settings.layout.committees.values().collect();
                    println!("{committees:?}");
                    solver(
                        &mut self.rng,
                        self.id,
                        &committees[..],
                        &self.settings.network,
                    )
                }
                None => {
                    println!("Unknown step: {step:?}");
                    Duration::ZERO
                }
            })
            .sum()
    }
}
