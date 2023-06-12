use std::collections::{BTreeMap, HashMap};

use consensus_engine::{Block, BlockId, Qc, StandardQc, View};
use proptest::prelude::*;
use proptest::test_runner::Config;
use proptest_state_machine::{prop_state_machine, ReferenceStateMachine, StateMachineTest};
use system_under_test::ConsensusEngineTest;

prop_state_machine! {
    #![proptest_config(Config {
        // Turn failure persistence off for demonstration. This means that no
        // regression file will be captured.
        failure_persistence: None,
        // Enable verbose mode to make the state machine test print the
        // transitions for each case.
        verbose: 1,
        // Only run 10 cases by default to avoid running out of system resources
        // and taking too long to finish.
        cases: 10000,
        .. Config::default()
    })]

    #[test]
    fn happy_path(sequential 1..100 => ConsensusEngineTest);
}

#[derive(Clone, Debug, Default)]
pub struct RefState {
    chain: BTreeMap<View, HashMap<BlockId, Block>>,
    blocks: HashMap<BlockId, Block>,
    highest_voted_view: View,
}

#[derive(Clone, Debug)]
pub enum Transition {
    ReceiveBlock(BlockId, BlockId),
    ApproveBlock(BlockId),
}

impl ReferenceStateMachine for RefState {
    type State = Self;

    type Transition = Transition;

    fn init_state() -> BoxedStrategy<Self::State> {
        let mut state = Self::default();
        let genesis_block = Block {
            view: 0,
            id: [0; 32],
            parent_qc: Qc::Standard(StandardQc::genesis()),
        };
        state.chain.insert(
            genesis_block.view,
            HashMap::from([(genesis_block.id, genesis_block.clone())]),
        );
        state.blocks.insert(genesis_block.id, genesis_block);

        Just(state).boxed()
    }

    fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        let blocks_in_last_two_views: Vec<Block> = state
            .chain
            .iter()
            .rev()
            .take(2)
            .map(|(_k, v)| v)
            .into_iter()
            .map(|m| m.values().cloned().collect::<Vec<Block>>())
            .flatten()
            .collect();
        let arb_parent = proptest::sample::select(blocks_in_last_two_views);

        let blocks_not_voted: Vec<Block> = state
            .chain
            .range(state.highest_voted_view + 1..)
            .map(|(_k, v)| v)
            .into_iter()
            .map(|m| m.values().cloned().collect::<Vec<Block>>())
            .flatten()
            .collect();

        if blocks_not_voted.len() == 0 {
            prop_oneof![
                1 => arb_parent.clone().prop_flat_map(move |parent|
                    (any::<BlockId>()).prop_map(move |id|
                        Transition::ReceiveBlock(id, parent.id)
                    )
                ),
            ]
            .boxed()
        } else {
            let arb_block_not_voted = proptest::sample::select(blocks_not_voted);
            prop_oneof![
                2 => arb_parent.clone().prop_flat_map(move |parent|
                    (any::<BlockId>()).prop_map(move |id|
                        Transition::ReceiveBlock(id, parent.id)
                    )
                ),
                1 => arb_block_not_voted.clone().prop_map(move |block| Transition::ApproveBlock(block.id)),
            ].boxed()
        }
    }

    fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
        match transition {
            Transition::ReceiveBlock(id, qc_id) => {
                let qc_block = state.blocks.get(qc_id).unwrap();
                let new_block = Block {
                    id: *id,
                    view: qc_block.view + 1,
                    parent_qc: Qc::Standard(StandardQc {
                        view: qc_block.view,
                        id: qc_block.id,
                    }),
                };

                match state.chain.get_mut(&new_block.view) {
                    Some(blocks) => {
                        blocks.insert(new_block.id, new_block.clone());
                    }
                    None => {
                        state.chain.insert(
                            new_block.view,
                            HashMap::from([(new_block.id, new_block.clone())]),
                        );
                    }
                }
                state.blocks.insert(new_block.id, new_block);
            }
            Transition::ApproveBlock(id) => {
                let block = state.blocks.get(id).unwrap();
                state.highest_voted_view = block.view;
            }
        }

        state
    }

    fn preconditions(state: &Self::State, transition: &Self::Transition) -> bool {
        match transition {
            Transition::ReceiveBlock(_id, qc_id) => state.blocks.contains_key(qc_id),
            Transition::ApproveBlock(id) => {
                if let Some(block) = state.blocks.get(id) {
                    state.highest_voted_view < block.view
                } else {
                    false
                }
            }
        }
    }
}

impl StateMachineTest for ConsensusEngineTest {
    type SystemUnderTest = Self;

    type Reference = RefState;

    fn init_test(
        _ref_state: &<Self::Reference as proptest_state_machine::ReferenceStateMachine>::State,
    ) -> Self::SystemUnderTest {
        ConsensusEngineTest::new()
    }

    fn apply(
        state: Self::SystemUnderTest,
        ref_state: &<Self::Reference as proptest_state_machine::ReferenceStateMachine>::State,
        transition: <Self::Reference as proptest_state_machine::ReferenceStateMachine>::Transition,
    ) -> Self::SystemUnderTest {
        match transition {
            Transition::ReceiveBlock(id, _qc_id) => {
                let engine = state
                    .engine
                    .receive_block(ref_state.blocks.get(&id).unwrap().clone())
                    .unwrap();

                ConsensusEngineTest { engine }
            }
            Transition::ApproveBlock(id) => {
                let (engine, _send) = state
                    .engine
                    .approve_block(ref_state.blocks.get(&id).unwrap().clone());

                //TODO: assert 'send'

                ConsensusEngineTest { engine }
            }
        }
    }

    fn check_invariants(
        state: &Self::SystemUnderTest,
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) {
        let mut latest_committed_view = 0; // genesis
        if ref_state.chain.len() >= 3 {
            let (view, _) = ref_state.chain.iter().rev().nth(2).unwrap();
            latest_committed_view = *view;
        }
        assert_eq!(state.engine.latest_committed_view(), latest_committed_view);

        //TODO:: assert other fields in the state
    }
}

mod system_under_test {
    use consensus_engine::*;

    #[derive(Clone, Debug)]
    pub struct ConsensusEngineTest {
        pub engine: Carnot<MockOverlay>,
    }

    impl ConsensusEngineTest {
        pub fn new() -> Self {
            let engine = Carnot::from_genesis(
                [0; 32],
                Block {
                    view: 0,
                    id: [0; 32],
                    parent_qc: Qc::Standard(StandardQc::genesis()),
                },
                MockOverlay,
            );

            ConsensusEngineTest { engine }
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct MockOverlay;

    impl Overlay for MockOverlay {
        fn new(_nodes: Vec<NodeId>) -> Self {
            Self
        }

        fn root_committee(&self) -> Committee {
            vec![[0; 32]].into_iter().collect()
        }

        fn rebuild(&mut self, _timeout_qc: TimeoutQc) {
            todo!()
        }

        fn is_member_of_child_committee(&self, _parent: NodeId, _child: NodeId) -> bool {
            false
        }

        fn is_member_of_root_committee(&self, _id: NodeId) -> bool {
            true
        }

        fn is_member_of_leaf_committee(&self, _id: NodeId) -> bool {
            true
        }

        fn is_child_of_root_committee(&self, _id: NodeId) -> bool {
            false
        }

        fn node_committee(&self, _id: NodeId) -> Committee {
            self.root_committee()
        }

        fn parent_committee(&self, _id: NodeId) -> Committee {
            self.root_committee()
        }

        fn child_committees(&self, _id: NodeId) -> Vec<Committee> {
            vec![]
        }

        fn leaf_committees(&self, _id: NodeId) -> Vec<Committee> {
            vec![self.root_committee()]
        }

        fn leader(&self, _view: View) -> NodeId {
            [0; 32]
        }

        fn super_majority_threshold(&self, _id: NodeId) -> usize {
            todo!()
        }

        fn leader_super_majority_threshold(&self, _id: NodeId) -> usize {
            self.root_committee().len() * 2 / 3 + 1
        }
    }
}
