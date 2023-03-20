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

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
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

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CarnotStepSolverType {
    Plain(StepTime),
    ParentCommitteeReceiverSolver,
    ChildCommitteeReceiverSolver,
    LeaderToCommitteeReceiverSolver,
    RootCommitteeReceiverSolver,
}

impl CarnotStepSolverType {
    pub fn to_solver(self) -> CarnotStepSolver {
        match self {
            Self::Plain(time) => CarnotStepSolver::Plain(time),
            Self::ParentCommitteeReceiverSolver => {
                CarnotStepSolver::ParentCommitteeReceiverSolver(receive_proposal)
            }
            Self::ChildCommitteeReceiverSolver => {
                CarnotStepSolver::ChildCommitteeReceiverSolver(receive_commit)
            }
            Self::LeaderToCommitteeReceiverSolver => {
                CarnotStepSolver::LeaderToCommitteeReceiverSolver(receive_commit)
            }
            Self::RootCommitteeReceiverSolver => {
                CarnotStepSolver::RootCommitteeReceiverSolver(leader_receive_proposal)
            }
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CarnotNodeSettings {
    pub steps_costs: HashMap<CarnotStep, CarnotStepSolverType>,
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

pub const CARNOT_STEPS_COSTS: &[(CarnotStep, CarnotStepSolverType)] = &[
    (
        CarnotStep::RootReceiveProposal,
        CarnotStepSolverType::RootCommitteeReceiverSolver,
    ),
    (
        CarnotStep::ReceiveProposal,
        CarnotStepSolverType::ParentCommitteeReceiverSolver,
    ),
    (
        CarnotStep::ValidateProposal,
        CarnotStepSolverType::Plain(StepTime::from_secs(1)),
    ),
    (
        CarnotStep::LeaderReceiveVote,
        CarnotStepSolverType::LeaderToCommitteeReceiverSolver,
    ),
    (
        CarnotStep::ReceiveVote,
        CarnotStepSolverType::ChildCommitteeReceiverSolver,
    ),
    (
        CarnotStep::ValidateVote,
        CarnotStepSolverType::Plain(StepTime::from_secs(1)),
    ),
    (
        CarnotStep::Ignore,
        CarnotStepSolverType::Plain(StepTime::from_secs(0)),
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

        let mut overall_steps_time = Duration::ZERO;
        for step in steps.iter() {
            let step_time = match self.settings.steps_costs.get(step).unwrap().to_solver() {
                Plain(t) => t,
                ParentCommitteeReceiverSolver(solver) => {
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
                ChildCommitteeReceiverSolver(solver) => solver(
                    &mut self.rng,
                    self.id,
                    &self
                        .settings
                        .layout
                        .children_nodes(self.settings.layout.committee(self.id)),
                    &self.settings.network,
                ),
                RootCommitteeReceiverSolver(solver) => solver(
                    &mut self.rng,
                    self.id,
                    &self.settings.leaders,
                    &self.settings.network,
                ),
                LeaderToCommitteeReceiverSolver(solver) => {
                    let committees: Vec<&Committee> =
                        self.settings.layout.committees.values().collect();
                    solver(
                        &mut self.rng,
                        self.id,
                        &committees[..],
                        &self.settings.network,
                    )
                }
            };

            overall_steps_time += step_time
        }

        overall_steps_time
    }
}
