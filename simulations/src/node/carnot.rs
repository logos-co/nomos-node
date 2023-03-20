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
    fn(&mut SmallRng, NodeId, &Committee, &Network) -> StepTime;

pub type ChildCommitteeReceiverSolver =
    fn(&mut SmallRng, NodeId, &[&Committee], &Network) -> StepTime;

fn receive_proposal(
    rng: &mut SmallRng,
    node: NodeId,
    committee: &Committee,
    network: &Network,
) -> StepTime {
    assert!(!committee.is_empty());
    committee
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
                .iter()
                .filter_map(|&sender| network.send_message_cost(rng, sender, node))
                .max()
        })
        .max()
        .unwrap()
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
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

impl super::Step for CarnotStep {}

#[derive(Clone)]
pub enum CarnotStepSolver {
    Plain(StepTime),
    ParentCommitteeReceiverSolver(ParentCommitteeReceiverSolver),
    ChildCommitteeReceiverSolver(ChildCommitteeReceiverSolver),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CarnotStepSolverType {
    Plain(StepTime),
    ParentCommitteeReceiverSolver,
    ChildCommitteeReceiverSolver,
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
        }
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CarnotNodeSettings {
    pub steps_costs: HashMap<CarnotStep, CarnotStepSolverType>,
    pub network: Network,
    pub layout: Layout,
}

pub struct CarnotNode {
    id: NodeId,
    rng: SmallRng,
    settings: Rc<CarnotNodeSettings>,
}

pub const CARNOT_STEPS_COSTS: &[(CarnotStep, CarnotStepSolverType)] = &[
    (
        CarnotStep::ReceiveProposal,
        CarnotStepSolverType::ParentCommitteeReceiverSolver,
    ),
    (
        CarnotStep::ValidateProposal,
        CarnotStepSolverType::Plain(StepTime::from_secs(1)),
    ),
    (
        CarnotStep::ReceiveVote,
        CarnotStepSolverType::ChildCommitteeReceiverSolver,
    ),
    (
        CarnotStep::ValidateVote,
        CarnotStepSolverType::Plain(StepTime::from_secs(1)),
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
    type Settings = Rc<CarnotNodeSettings>;
    type Step = CarnotStep;

    fn new<R: Rng>(rng: &mut R, id: NodeId, settings: Self::Settings) -> Self {
        let rng = SmallRng::from_rng(rng).unwrap();
        Self { id, settings, rng }
    }

    fn id(&self) -> usize {
        self.id
    }

    fn run_step(&mut self, step: Self::Step) -> StepTime {
        use CarnotStepSolver::*;
        match self.settings.steps_costs.get(&step) {
            Some(step) => match step.to_solver() {
                Plain(t) => t,
                ParentCommitteeReceiverSolver(solver) => solver(
                    &mut self.rng,
                    self.id,
                    self.settings
                        .layout
                        .parent_nodes(self.settings.layout.committee(self.id)),
                    &self.settings.network,
                ),
                ChildCommitteeReceiverSolver(solver) => solver(
                    &mut self.rng,
                    self.id,
                    &self
                        .settings
                        .layout
                        .children_nodes(self.settings.layout.committee(self.id)),
                    &self.settings.network,
                ),
            },
            None => panic!("Unknown step: {step:?}"),
        }
    }
}
