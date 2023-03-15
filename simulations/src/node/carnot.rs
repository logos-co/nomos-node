// std
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;
// crates
use rand::prelude::SmallRng;
use rand::{Rng, SeedableRng};
// internal
use crate::network::Network;
use crate::node::{Node, NodeId, StepTime};
use crate::overlay::{Committee, Layout};

pub type ParentCommitteeReceiverSolver =
    fn(&mut SmallRng, NodeId, &Committee<CarnotNode>, &Network) -> StepTime;

pub type ChildCommitteeReceiverSolver =
    fn(&mut SmallRng, NodeId, &[&Committee<CarnotNode>], &Network) -> StepTime;

fn receive_proposal(
    rng: &mut SmallRng,
    node: NodeId,
    committee: &Committee<CarnotNode>,
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
    committees: &[&Committee<CarnotNode>],
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
    ReceiveProposal,
    ValidateProposal,
    ReceiveVote,
    ValidateVote,
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
}

pub struct CarnotNodeSettings {
    pub steps_costs: HashMap<CarnotStep, CarnotStepSolver>,
    pub network: Network,
    pub layout: Layout<CarnotNode>,
}

#[derive(Clone)]
pub struct CarnotNode {
    id: NodeId,
    rng: SmallRng,
    role: CarnotRole,
    settings: Rc<CarnotNodeSettings>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CarnotRole {
    Leader,
    Root,
    // missing role?
    Intermediate,
    Leaf,
}

pub const CARNOT_STEPS_COSTS: &[(CarnotStep, CarnotStepSolver)] = &[
    (
        CarnotStep::ReceiveProposal,
        CarnotStepSolver::ParentCommitteeReceiverSolver(receive_proposal),
    ),
    (
        CarnotStep::ValidateProposal,
        CarnotStepSolver::Plain(StepTime::from_secs(1)),
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

pub const CARNOT_LEADER_STEPS: &[CarnotStep] = &[CarnotStep::ReceiveVote, CarnotStep::ValidateVote];

pub const CARNOT_ROOT_STEPS: &[CarnotStep] = &[
    CarnotStep::ReceiveProposal,
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

impl Node for CarnotNode {
    type Role = CarnotRole;
    type Settings = Rc<CarnotNodeSettings>;
    type Step = CarnotStep;

    fn new<R: Rng>(rng: &mut R, id: NodeId, role: Self::Role, settings: Self::Settings) -> Self {
        let rng = SmallRng::from_rng(rng).unwrap();
        Self {
            id,
            rng,
            role,
            settings,
        }
    }

    fn id(&self) -> usize {
        self.id
    }

    fn run_step(&mut self, step: Self::Step) -> StepTime {
        use CarnotStepSolver::*;
        match self.settings.steps_costs.get(&step) {
            Some(Plain(t)) => *t,
            Some(ParentCommitteeReceiverSolver(solver)) => solver(
                &mut self.rng,
                self.id,
                self.settings
                    .layout
                    .parent_nodes(self.settings.layout.committee(self.id)),
                &self.settings.network,
            ),
            Some(ChildCommitteeReceiverSolver(solver)) => solver(
                &mut self.rng,
                self.id,
                &self
                    .settings
                    .layout
                    .children_nodes(self.settings.layout.committee(self.id)),
                &self.settings.network,
            ),
            None => {
                panic!("Unknown step: {step:?}");
            }
        }
    }
}
