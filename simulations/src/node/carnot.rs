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

#[derive(Debug, Eq, PartialEq, Hash)]
pub enum CarnotStep {
    ReceiveProposal,
    ValidateProposal,
    ReceiveVote,
    ValidateVote,
}

pub enum CarnotStepSolver {
    Plain(StepTime),
    ParentCommitteeReceiverSolver(ParentCommitteeReceiverSolver),
    ChildCommitteeReceiverSolver(ChildCommitteeReceiverSolver),
}

pub struct CarnotNodeSettings {
    steps_costs: HashMap<CarnotStep, CarnotStepSolver>,
    network: Network,
    layout: Layout,
}

pub struct CarnotNode {
    id: NodeId,
    rng: SmallRng,
    settings: Rc<CarnotNodeSettings>,
}

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
            Some(Plain(t)) => *t,
            Some(ParentCommitteeReceiverSolver(solver)) => solver(
                &mut self.rng,
                self.id,
                self.settings.layout.parent_nodes(self.id),
                &self.settings.network,
            ),
            Some(ChildCommitteeReceiverSolver(solver)) => solver(
                &mut self.rng,
                self.id,
                &self.settings.layout.children_nodes(self.id),
                &self.settings.network,
            ),
            None => {
                panic!("Unknown step: {:?}", step);
            }
        }
    }
}
