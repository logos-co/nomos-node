use std::{
    collections::{BTreeMap, HashSet},
    panic,
};

use consensus_engine::{Block, LeaderProof, Qc, StandardQc, View};
use proptest::prelude::*;
use proptest::test_runner::Config;
use proptest_state_machine::{prop_state_machine, ReferenceStateMachine, StateMachineTest};
use system_under_test::ConsensusEngineTest;

prop_state_machine! {
    #![proptest_config(Config {
        // Only run 100 cases by default to avoid running out of system resources
        // and taking too long to finish.
        cases: 100,
        .. Config::default()
    })]

    #[test]
    // run 50 state transitions per test case
    fn happy_path(sequential 1..50 => ConsensusEngineTest);
}

// A reference state machine (RefState) is used to generated state transitions.
// To generate some kinds of transition, we may need to keep historical blocks in RefState.
// Also, RefState can be used to check invariants of the real state machine in some cases.
//
// We should try to design this reference state as simple/intuitive as possible,
// so that we don't need to replicate the logic implemented in consensus-engine.
#[derive(Clone, Debug)]
pub struct RefState {
    chain: BTreeMap<View, HashSet<Block>>,
    highest_voted_view: View,
}

// State transtitions that will be picked randomly
#[derive(Clone, Debug)]
pub enum Transition {
    Nop,
    ReceiveBlock(Block),
    ReceiveUnsafeBlock(Block),
    ApproveBlock(Block),
    ApprovePastBlock(Block),
    //TODO: add more transitions
}

const LEADER_PROOF: LeaderProof = LeaderProof::LeaderId { leader_id: [0; 32] };
const INITIAL_HIGHEST_VOTED_VIEW: View = -1;

impl ReferenceStateMachine for RefState {
    type State = Self;

    type Transition = Transition;

    // Initialize the reference state machine
    fn init_state() -> BoxedStrategy<Self::State> {
        let genesis_block = Block {
            view: 0,
            id: [0; 32],
            parent_qc: Qc::Standard(StandardQc::genesis()),
            leader_proof: LEADER_PROOF.clone(),
        };

        Just(RefState {
            chain: BTreeMap::from([(genesis_block.view, HashSet::from([genesis_block.clone()]))]),
            highest_voted_view: INITIAL_HIGHEST_VOTED_VIEW,
        })
        .boxed()
    }

    // Generate transitions based on the current reference state machine
    fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        // Instead of using verbose `if` statements here to filter out the types of transitions
        // which cannot be created based on the current reference state,
        // each `state.transition_*` function returns a Nop transition
        // if it cannot generate the promised transition for the current reference state.
        // Both reference and real state machine do nothing for Nop transitions.
        prop_oneof![
            3 => state.transition_receive_block(),
            2 => state.transition_receive_unsafe_block(),
            2 => state.transition_approve_block(),
            1 => state.transition_approve_past_block(),
        ]
        .boxed()
    }

    // Check if the transition is valid for a given reference state, before applying the transition
    // If invalid, the transition will be ignored and a new transition will be generated.
    //
    // Also, preconditions are used for shrinking in failure cases.
    // Preconditions check if the transition is still valid after some shrinking is applied.
    // If the transition became invalid for the shrinked state, the shrinking is stopped or
    // is continued to other directions.
    fn preconditions(state: &Self::State, transition: &Self::Transition) -> bool {
        // In most cases, we need to check the same conditions again used to create transitions.
        // This is redundant for success cases, but is necessary for shrinking in failure cases,
        // because some transitions may no longer be valid after some shrinking is applied.
        match transition {
            Transition::Nop => true,
            Transition::ReceiveBlock(block) => block.view >= state.current_view(),
            Transition::ReceiveUnsafeBlock(block) => block.view < state.current_view(),
            Transition::ApproveBlock(block) => state.highest_voted_view < block.view,
            Transition::ApprovePastBlock(block) => state.highest_voted_view >= block.view,
        }
    }

    // Apply the given transition on the reference state machine,
    // so that it can be used to generate next transitions.
    fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
        match transition {
            Transition::Nop => {}
            Transition::ReceiveBlock(block) => {
                state
                    .chain
                    .entry(block.view)
                    .or_default()
                    .insert(block.clone());
            }
            Transition::ReceiveUnsafeBlock(_) => {
                // Nothing to do because we expect the state doesn't change.
            }
            Transition::ApproveBlock(block) => {
                state.highest_voted_view = block.view;
            }
            Transition::ApprovePastBlock(_) => {
                // Nothing to do because we expect the state doesn't change.
            }
        }

        state
    }
}

impl RefState {
    // Generate a Transition::ReceiveBlock.
    fn transition_receive_block(&self) -> BoxedStrategy<Transition> {
        let recent_parents = self
            .chain
            .range(self.current_view() - 1..)
            .flat_map(|(_view, blocks)| blocks.iter().cloned())
            .collect::<Vec<Block>>();

        if recent_parents.is_empty() {
            Just(Transition::Nop).boxed()
        } else {
            // proptest::sample::select panics if the input is empty
            proptest::sample::select(recent_parents)
                .prop_map(move |parent| -> Transition {
                    Transition::ReceiveBlock(Self::consecutive_block(&parent))
                })
                .boxed()
        }
    }

    // Generate a Transition::ReceiveUnsafeBlock.
    fn transition_receive_unsafe_block(&self) -> BoxedStrategy<Transition> {
        let old_parents = self
            .chain
            .range(..self.current_view() - 1)
            .flat_map(|(_view, blocks)| blocks.iter().cloned())
            .collect::<Vec<Block>>();

        if old_parents.is_empty() {
            Just(Transition::Nop).boxed()
        } else {
            // proptest::sample::select panics if the input is empty
            proptest::sample::select(old_parents)
                .prop_map(move |parent| -> Transition {
                    Transition::ReceiveUnsafeBlock(Self::consecutive_block(&parent))
                })
                .boxed()
        }
    }

    // Generate a Transition::ApproveBlock.
    fn transition_approve_block(&self) -> BoxedStrategy<Transition> {
        let blocks_not_voted = self
            .chain
            .range(self.highest_voted_view + 1..)
            .flat_map(|(_view, blocks)| blocks.iter().cloned())
            .collect::<Vec<Block>>();

        if blocks_not_voted.is_empty() {
            Just(Transition::Nop).boxed()
        } else {
            // proptest::sample::select panics if the input is empty
            proptest::sample::select(blocks_not_voted)
                .prop_map(move |block| Transition::ApproveBlock(block))
                .boxed()
        }
    }

    // Generate a Transition::ApprovePastBlock.
    fn transition_approve_past_block(&self) -> BoxedStrategy<Transition> {
        let past_blocks = self
            .chain
            .range(INITIAL_HIGHEST_VOTED_VIEW..self.highest_voted_view)
            .flat_map(|(_view, blocks)| blocks.iter().cloned())
            .collect::<Vec<Block>>();

        if past_blocks.is_empty() {
            Just(Transition::Nop).boxed()
        } else {
            // proptest::sample::select panics if the input is empty
            proptest::sample::select(past_blocks)
                .prop_map(move |block| Transition::ApprovePastBlock(block))
                .boxed()
        }
    }

    fn current_view(&self) -> View {
        let (last_view, _) = self.chain.last_key_value().unwrap();
        // TODO: this logic must cover other cases for unhappy path
        last_view.clone()
    }

    fn consecutive_block(parent: &Block) -> Block {
        Block {
            // use rand because we don't want this to be shrinked by proptest
            id: rand::thread_rng().gen(),
            view: parent.view + 1,
            parent_qc: Qc::Standard(StandardQc {
                view: parent.view,
                id: parent.id,
            }),
            leader_proof: LEADER_PROOF.clone(),
        }
    }
}

// StateMachineTest defines how transitions are applied to the real state machine
// and what checks should be performed.
impl StateMachineTest for ConsensusEngineTest {
    // SUT is the real state machine that we want to test.
    type SystemUnderTest = Self;

    // A reference state machine that should be compared against the SUT.
    type Reference = RefState;

    // Initialize the SUT state
    fn init_test(
        _ref_state: &<Self::Reference as proptest_state_machine::ReferenceStateMachine>::State,
    ) -> Self::SystemUnderTest {
        ConsensusEngineTest::new()
    }

    // Apply the transition on the SUT state and check post-conditions
    fn apply(
        state: Self::SystemUnderTest,
        _ref_state: &<Self::Reference as proptest_state_machine::ReferenceStateMachine>::State,
        transition: <Self::Reference as proptest_state_machine::ReferenceStateMachine>::Transition,
    ) -> Self::SystemUnderTest {
        println!("{transition:?}");

        match transition {
            Transition::Nop => state,
            Transition::ReceiveBlock(block) => {
                let engine = state.engine.receive_block(block.clone()).unwrap();
                assert!(engine.blocks_in_view(block.view).contains(&block));

                ConsensusEngineTest { engine }
            }
            Transition::ReceiveUnsafeBlock(block) => {
                let result = panic::catch_unwind(|| state.engine.receive_block(block));
                assert!(result.is_err() || result.unwrap().is_err());

                state
            }
            Transition::ApproveBlock(block) => {
                let (engine, _) = state.engine.approve_block(block.clone());
                assert_eq!(engine.highest_voted_view(), block.view);

                ConsensusEngineTest { engine }
            }
            Transition::ApprovePastBlock(block) => {
                let result = panic::catch_unwind(|| state.engine.approve_block(block));
                assert!(result.is_err());

                state
            }
        }
    }

    // Check invariants after every transition.
    // We have the option to use RefState for comparison in some cases.
    fn check_invariants(
        state: &Self::SystemUnderTest,
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) {
        //TODO: this may be false in unhappy path
        assert_eq!(state.engine.current_view(), ref_state.current_view());

        assert_eq!(
            state.engine.highest_voted_view(),
            ref_state.highest_voted_view
        );

        //TODO: add more invariants with more public functions of Carnot
    }
}

// SUT is a system (state) that we want to test.
mod system_under_test {
    use consensus_engine::{
        overlay::{FlatOverlay, RoundRobin, Settings},
        *,
    };

    use crate::LEADER_PROOF;

    #[derive(Clone, Debug)]
    pub struct ConsensusEngineTest {
        pub engine: Carnot<FlatOverlay<RoundRobin>>,
    }

    impl ConsensusEngineTest {
        pub fn new() -> Self {
            let engine = Carnot::from_genesis(
                [0; 32],
                Block {
                    view: 0,
                    id: [0; 32],
                    parent_qc: Qc::Standard(StandardQc::genesis()),
                    leader_proof: LEADER_PROOF.clone(),
                },
                FlatOverlay::new(Settings {
                    nodes: vec![[0; 32]],
                    leader: RoundRobin::default(),
                }),
            );

            ConsensusEngineTest { engine }
        }
    }
}
