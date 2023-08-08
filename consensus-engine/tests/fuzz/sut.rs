use std::{collections::HashSet, panic};

use consensus_engine::overlay::FreezeMembership;
use consensus_engine::{
    overlay::{FlatOverlay, FlatOverlaySettings, RoundRobin},
    *,
};
use proptest_state_machine::{ReferenceStateMachine, StateMachineTest};

use crate::fuzz::ref_state::RefState;
use crate::fuzz::transition::Transition;

// ConsensusEngineTest defines a state that we want to test.
// This is called as SUT (System Under Test).
#[derive(Clone, Debug)]
pub struct ConsensusEngineTest {
    pub engine: Carnot<FlatOverlay<RoundRobin, FreezeMembership>>,
}

impl ConsensusEngineTest {
    pub fn new() -> Self {
        let engine = Carnot::from_genesis(
            NodeId::new([0; 32]),
            Block {
                view: View::new(0),
                id: BlockId::zeros(),
                parent_qc: Qc::Standard(StandardQc::genesis()),
                leader_proof: LeaderProof::LeaderId {
                    leader_id: NodeId::new([0; 32]),
                },
            },
            FlatOverlay::new(FlatOverlaySettings {
                nodes: vec![NodeId::new([0; 32])],
                leader: RoundRobin::default(),
                leader_super_majority_threshold: None,
            }),
        );

        ConsensusEngineTest { engine }
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
            Transition::ReceiveSafeBlock(block) => {
                // Because we generate/apply transitions sequentially,
                // this transition will always be applied to the same state that it was generated against.
                // In other words, we can assume that the `block` is always safe for the current state.
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
            Transition::LocalTimeout => {
                let (engine, _) = state.engine.local_timeout();
                assert_eq!(engine.highest_voted_view(), engine.current_view());

                ConsensusEngineTest { engine }
            }
            Transition::ReceiveTimeoutQcForRecentView(timeout_qc) => {
                let engine = state.engine.receive_timeout_qc(timeout_qc.clone());
                assert_eq!(engine.current_view(), RefState::new_view_from(&timeout_qc));

                ConsensusEngineTest { engine }
            }
            Transition::ReceiveTimeoutQcForOldView(timeout_qc) => {
                #[allow(clippy::redundant_clone)]
                let prev_engine = state.engine.clone();
                let engine = state.engine.receive_timeout_qc(timeout_qc);

                // Check that the engine state didn't change.
                assert_eq!(engine, prev_engine);

                ConsensusEngineTest { engine }
            }
            Transition::ApproveNewViewWithLatestTimeoutQc(timeout_qc, new_views) => {
                let (engine, _) = state.engine.approve_new_view(timeout_qc.clone(), new_views);
                assert_eq!(
                    engine.highest_voted_view(),
                    RefState::new_view_from(&timeout_qc)
                );

                ConsensusEngineTest { engine }
            }
        }
    }

    // Check invariants after every transition.
    // We have the option to use RefState for comparison in some cases.
    fn check_invariants(
        state: &Self::SystemUnderTest,
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) {
        assert_eq!(state.engine.current_view(), ref_state.current_view());
        assert_eq!(
            state.engine.highest_voted_view(),
            ref_state.highest_voted_view
        );
        assert_eq!(state.engine.high_qc().view, ref_state.high_qc().view);

        match state.engine.last_view_timeout_qc() {
            Some(timeout_qc) => assert!(ref_state.latest_timeout_qcs().contains(&timeout_qc)),
            None => assert!(ref_state.latest_timeout_qcs().is_empty()),
        }

        // Check if state and ref_state have the same blocks
        let ref_blocks: HashSet<&Block> = ref_state
            .chain
            .values()
            .flat_map(|entry| entry.blocks.iter())
            .collect();
        let blocks: HashSet<&Block> = state.engine.safe_blocks().values().collect();
        assert_eq!(blocks, ref_blocks);
    }
}
