// 1 - Leader forwards a proposal - Leader builds proposal
// 2 - Every committee member receives the proposal and validates it
//

use crate::node::{CommitteeId, Node, NodeId, StepTime};
use crate::overlay::Overlay;
use rand::{Rng, RngCore, SeedableRng};
use std::time::Duration;

pub struct ConsensusRunner<N> {
    nodes: Vec<N>,
}

#[derive(Debug)]
pub struct Report {
    round_time: Duration,
}

#[derive(Copy, Clone)]
pub enum LayoutNodes {
    Leader,
    Committee,
    LeafCommittee,
}

impl<N: Node> ConsensusRunner<N>
where
    N::Settings: Clone,
{
    pub fn new<R: Rng>(mut rng: R, size: usize, node_settings: N::Settings) -> Self {
        let nodes = (0..size)
            .map(|id| N::new(&mut rng, id, node_settings.clone()))
            .collect();
        Self { nodes }
    }

    pub fn node_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.iter().map(Node::id)
    }

    pub fn run<O: Overlay, R: Rng>(
        &mut self,
        rng: &mut R,
        mut overlay: O,
        execution: &[(LayoutNodes, N::Step, Box<dyn Fn(&[StepTime]) -> StepTime>)],
    ) -> Report
    where
        N::Step: Clone,
    {
        let node_ids = self.nodes.iter().map(|node| node.id()).collect::<Vec<_>>();

        let leaders: Vec<_> = overlay.leaders(&node_ids, 1, rng).collect();
        let layout = overlay.layout(&node_ids, rng);

        let round_time = execution
            .iter()
            .map(|(layout_node, step, reducer)| {
                let times: Vec<StepTime> = match layout_node {
                    LayoutNodes::Leader => leaders
                        .iter()
                        .map(|&leader| self.nodes[leader].run_step(step.clone()))
                        .collect(),
                    LayoutNodes::Committee => {
                        let non_leaf_committees = layout
                            .children
                            .iter()
                            .filter_map(|(id, children)| (!children.is_empty()).then_some(id));

                        non_leaf_committees
                            .flat_map(|committee_id| {
                                layout
                                    .committees
                                    .get(committee_id)
                                    .unwrap()
                                    .iter()
                                    .map(|&node| self.nodes[node].run_step(step.clone()))
                                    .max()
                            })
                            .collect()
                    }
                    LayoutNodes::LeafCommittee => {
                        let leaf_committees = layout
                            .children
                            .iter()
                            .filter_map(|(id, children)| children.is_empty().then_some(id));

                        leaf_committees
                            .flat_map(|committee_id| {
                                layout
                                    .committees
                                    .get(committee_id)
                                    .unwrap()
                                    .iter()
                                    .map(|&node| self.nodes[node].run_step(step.clone()))
                                    .max()
                            })
                            .collect()
                    }
                };

                reducer(&times)
            })
            .sum();

        Report { round_time }
    }
}

#[cfg(test)]
mod test {
    use crate::network::behaviour::NetworkBehaviour;
    use crate::network::regions::{Region, RegionsData};
    use crate::network::Network;
    use crate::node::carnot::{
        CarnotNode, CarnotNodeSettings, CARNOT_LEADER_STEPS, CARNOT_STEPS_COSTS,
    };
    use crate::node::{NodeId, StepTime};
    use crate::overlay::flat::FlatOverlay;
    use crate::overlay::Overlay;
    use crate::runner::{ConsensusRunner, LayoutNodes};
    use rand::rngs::SmallRng;
    use rand::SeedableRng;
    use std::rc::Rc;
    use std::time::Duration;

    #[test]
    fn test_run_flat() {
        let regions = std::iter::once((Region::Europe, (0..10).collect())).collect();
        let network_behaviour = std::iter::once((
            (Region::Europe, Region::Europe),
            NetworkBehaviour::new(Duration::from_millis(100), 0.0),
        ))
        .collect();
        let overlay = FlatOverlay::new(());
        let mut rng = SmallRng::seed_from_u64(0);
        let node_ids: Vec<NodeId> = (0..10).collect();
        // let nodes_ids
        let node_settings: CarnotNodeSettings = CarnotNodeSettings {
            steps_costs: CARNOT_STEPS_COSTS.iter().cloned().collect(),
            network: Network::new(RegionsData::new(regions, network_behaviour)),
            layout: overlay.layout(&node_ids, &mut rng),
        };

        let mut runner: ConsensusRunner<CarnotNode> =
            ConsensusRunner::new(&mut rng, 10, Rc::new(node_settings));

        let carnot_steps: Vec<_> = CARNOT_LEADER_STEPS
            .iter()
            .copied()
            .map(|step| {
                (
                    LayoutNodes::Leader,
                    step,
                    Box::new(|times: &[StepTime]| *times.iter().max().unwrap())
                        as Box<dyn Fn(&[StepTime]) -> StepTime>,
                )
            })
            .collect();

        println!("{:?}", runner.run(&mut rng, overlay, &carnot_steps));
    }
}
