// 1 - Leader forwards a proposal - Leader builds proposal
// 2 - Every committee member receives the proposal and validates it
//

use crate::node::{Node, NodeId, StepTime};
use crate::overlay::Layout;
use rand::Rng;
use std::time::Duration;

pub struct ConsensusRunner<N: Node> {
    nodes: Vec<N>,
    layout: Layout<N>,
    leaders: Vec<NodeId>,
}

#[allow(dead_code)]
#[derive(Debug, serde::Serialize)]
pub struct Report {
    round_time: Duration,
}

#[derive(Copy, Clone)]
pub enum LayoutNodes {
    Leader,
    Committee,
    LeafCommittee,
}

pub type ExecutionSteps<S> = [(LayoutNodes, S, Box<dyn Fn(&[StepTime]) -> StepTime>)];

impl<N: Node> ConsensusRunner<N>
where
    N::Settings: Clone,
{
    pub fn new<R: Rng>(
        mut rng: R,
        layout: Layout<N>,
        leaders: Vec<NodeId>,
        node_settings: N::Settings,
    ) -> Self {
        let nodes = layout
            .node_ids()
            .map(|id| {
                let c = &layout.committees[&layout.committee(id)];
                N::new(&mut rng, id, c.role.clone(), node_settings.clone())
            })
            .collect();
        Self {
            nodes,
            layout,
            leaders,
        }
    }

    pub fn node_ids(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes.iter().map(Node::id)
    }

    pub fn run(&mut self, reducer: Box<dyn Fn(&[StepTime]) -> StepTime>) -> Report
    where
        N::Step: Clone,
    {
        let leaders = &self.leaders;
        let layout = &self.layout;

        //let round_time = layout
        //    .node_ids()
        //    .map(|id| {
        //        // get an actual carnotnode.
        //        self.nodes[id].run_step();
        //    }).collect

        Report { round_time }
    }
}

#[cfg(test)]
mod test {
    use crate::network::behaviour::NetworkBehaviour;
    use crate::network::regions::{Region, RegionsData};
    use crate::network::Network;
    use crate::node::carnot::{
        CarnotNode, CarnotNodeSettings, CarnotRole, CARNOT_INTERMEDIATE_STEPS, CARNOT_LEADER_STEPS,
        CARNOT_LEAF_STEPS, CARNOT_ROOT_STEPS, CARNOT_STEPS_COSTS,
    };
    use crate::node::{NodeId, StepTime};
    use crate::overlay::flat::FlatOverlay;
    use crate::overlay::tree::{TreeOverlay, TreeSettings, TreeType};
    use crate::overlay::Overlay;
    use crate::runner::{ConsensusRunner, LayoutNodes};
    use rand::rngs::SmallRng;
    use rand::{Rng, SeedableRng};
    use std::rc::Rc;
    use std::time::Duration;

    fn setup_runner<R: Rng, O: Overlay<CarnotNode>>(
        mut rng: &mut R,
        overlay: &O,
    ) -> ConsensusRunner<CarnotNode> {
        let regions = std::iter::once((Region::Europe, (0..10).collect())).collect();
        let network_behaviour = std::iter::once((
            (Region::Europe, Region::Europe),
            NetworkBehaviour::new(Duration::from_millis(100), 0.0),
        ))
        .collect();
        let node_ids: Vec<NodeId> = (0..10).collect();
        let layout = overlay.layout(&node_ids, &mut rng);
        let leaders = overlay.leaders(&node_ids, 1, &mut rng).collect();
        let node_settings: CarnotNodeSettings = CarnotNodeSettings {
            steps_costs: CARNOT_STEPS_COSTS.iter().cloned().collect(),
            network: Network::new(RegionsData::new(regions, network_behaviour)),
            layout: overlay.layout(&node_ids, &mut rng),
        };

        ConsensusRunner::new(&mut rng, layout, leaders, Rc::new(node_settings))
    }

    #[test]
    fn run_flat_single_leader_steps() {
        let mut rng = SmallRng::seed_from_u64(0);
        let overlay = FlatOverlay::new(());

        let mut runner = setup_runner(&mut rng, &overlay);

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

        assert_eq!(
            Duration::from_millis(1100),
            runner
                .run(Box::new(|times: &[StepTime]| *times.iter().max().unwrap())
                    as Box<dyn Fn(&[StepTime]) -> StepTime>)
                .round_time
        );
    }

    #[test]
    fn run_flat_single_leader_single_committee() {
        let mut rng = SmallRng::seed_from_u64(0);
        let overlay = FlatOverlay::new(());

        let mut runner: ConsensusRunner<CarnotNode> = setup_runner(&mut rng, &overlay);

        let leader_steps = CARNOT_LEADER_STEPS.iter().copied().map(|step| {
            (
                LayoutNodes::Leader,
                step,
                Box::new(|times: &[StepTime]| *times.iter().max().unwrap())
                    as Box<dyn Fn(&[StepTime]) -> StepTime>,
            )
        });

        let committee_steps = CARNOT_LEAF_STEPS.iter().copied().map(|step| {
            (
                LayoutNodes::LeafCommittee,
                step,
                Box::new(|times: &[StepTime]| *times.iter().max().unwrap())
                    as Box<dyn Fn(&[StepTime]) -> StepTime>,
            )
        });

        let carnot_steps: Vec<_> = leader_steps.chain(committee_steps).collect();

        assert_eq!(
            Duration::from_millis(2200),
            runner
                .run(Box::new(|times: &[StepTime]| *times.iter().max().unwrap())
                    as Box<dyn Fn(&[StepTime]) -> StepTime>)
                .round_time
        );
    }

    #[test]
    fn run_tree_committee() {
        let mut rng = SmallRng::seed_from_u64(0);

        let overlay = TreeOverlay::new(TreeSettings {
            tree_type: TreeType::FullBinaryTree,
            depth: 10,
            committee_size: 10,
        });

        let mut runner: ConsensusRunner<CarnotNode> = setup_runner(&mut rng, &overlay);
        let committee_steps = CARNOT_LEAF_STEPS.iter().copied().map(|step| {
            (
                LayoutNodes::LeafCommittee,
                step,
                Box::new(|times: &[StepTime]| *times.iter().max().unwrap())
                    as Box<dyn Fn(&[StepTime]) -> StepTime>,
            )
        });
        let carnot_steps: Vec<_> = committee_steps.collect();

        assert_eq!(
            Duration::from_millis(200),
            runner
                .run(Box::new(|times: &[StepTime]| *times.iter().max().unwrap())
                    as Box<dyn Fn(&[StepTime]) -> StepTime>)
                .round_time
        );
    }
}
