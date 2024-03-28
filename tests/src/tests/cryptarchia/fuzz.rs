use std::collections::HashMap;

use cryptarchia_consensus::{leadership::Leader, Cryptarchia, Error};
use cryptarchia_engine::Slot;
use cryptarchia_ledger::{Coin, LedgerState};
use nomos_core::header::{
    cryptarchia::{Builder, Header},
    HeaderId,
};
use proptest::{
    strategy::{BoxedStrategy, Just, Strategy},
    test_runner::Config,
};
use proptest_state_machine::{prop_state_machine, ReferenceStateMachine, StateMachineTest};

prop_state_machine! {
    #![proptest_config(Config {
        // Only run 10 cases by default to avoid running out of system resources
        // and taking too long to finish.
        cases: 10,
        .. Config::default()
    })]

    #[test]
    // Run max 100 state transitions per test case
    fn cryptarchia_ledger_test(sequential 1..100 => FollowerNode);
}

const GENESIS_ID: [u8; 32] = [0; 32];
const SECRET_KEY: [u8; 32] = [0; 32];
const INITIAL_COIN_NONCE: [u8; 32] = [0; 32];
const TOTAL_STAKE: u32 = 1;
const INITIAL_SLOT: u64 = 0;
const MAX_UNBROADCASTED_BLOCKS: usize = 5;

#[derive(Clone, Debug)]
enum Transition {
    ProcessBlock(Header),
}

// A leader node that proposes all new blocks.
// This is used as a reference state that generates a sequence of transitions
// and can be compared against the SUT (the state machine that we want to test).
#[derive(Debug, Clone)]
struct LeaderNode {
    cryptarchia: Cryptarchia,
    leader: Leader,
    next_slot: Slot,
    unbroadcasted_blocks: HashMap<HeaderId, Header>,
}

impl ReferenceStateMachine for LeaderNode {
    type State = Self;
    type Transition = Transition;

    // Initialize the reference state machine
    fn init_state() -> BoxedStrategy<Self::State> {
        let (cryptarchia, leader) = init();
        let mut leader_node = Self {
            cryptarchia,
            leader,
            next_slot: (INITIAL_SLOT + 1).into(),
            unbroadcasted_blocks: HashMap::new(),
        };

        // Prepare several blocks in advance, so that they can be sent to the SUT (follower node)
        // in random order. For details, see the `transitions` function below.
        for _ in 0..MAX_UNBROADCASTED_BLOCKS {
            leader_node.prepare_new_block();
        }

        Just(leader_node).boxed()
    }

    // Generate transitions based on the current reference state machine
    fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        state.transition_process_block()
    }

    // Check if the transition is valid for a given reference state, before applying the transition
    // If invalid, the transition will be ignored and a new transition will be generated.
    //
    // In most cases, this will return true as the transition was generated as intended by the
    // `transitions` function. However, this also checks if the transition is still valid during
    // shrinking in failure cases. If the transition becomes invalid for the shrunk state,
    // the shrinking is stopped or continued to other directions.
    fn preconditions(state: &Self::State, transition: &Self::Transition) -> bool {
        match transition {
            Transition::ProcessBlock(header) => {
                state.unbroadcasted_blocks.contains_key(&header.id())
            }
        }
    }

    // Apply the given transition on the reference state machine,
    // so that it can be used to generate next transitions.
    fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
        match transition {
            Transition::ProcessBlock(header) => {
                // Remove the block from the unbroadcasted blocks.
                state.unbroadcasted_blocks.remove(&header.id());
                // Prepare a new block for the next slot.
                state.prepare_new_block();
                state
            }
        }
    }
}

impl LeaderNode {
    fn prepare_new_block(&mut self) {
        // Propose a new block. This must be always succeeded.
        let slot = self.next_slot;
        let parent = self.cryptarchia.tip();
        let epoch_state = self.cryptarchia.epoch_state_for_slot(slot).unwrap();
        // Assume that the leader's coin is always eligible to lead the slot.
        let proof = self
            .leader
            .build_proof_for(epoch_state, slot, parent)
            .unwrap();
        // Generate a new block with no content for simplicity.
        let header = Builder::new(parent, proof).build([0; 32].into(), 0);

        // Process the new block immediately in the leader node.
        // This must be always succeeded.
        self.cryptarchia = self.cryptarchia.try_apply_header(&header).unwrap();
        self.leader.follow_chain(
            header.parent(),
            header.id(),
            *header.leader_proof().commitment(),
        );

        // Keep the new block in the queue to broadcast later
        self.unbroadcasted_blocks.insert(header.id(), header);
        // Advance time
        self.next_slot = self.next_slot + 1;
    }

    // Generate a Transition::ProcessBlock by choosing a block randomly from unbroadcasted blocks.
    // The selected block will be sent to the SUT (follower node).
    // The random selection simulates unpredictable network delays in p2p communication (even with mixnet).
    fn transition_process_block(&self) -> BoxedStrategy<Transition> {
        let unbroadcasted_blocks = self.unbroadcasted_blocks.clone();
        proptest::sample::select(
            self.unbroadcasted_blocks
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
        )
        .prop_map(move |header_id| {
            Transition::ProcessBlock(unbroadcasted_blocks[&header_id].clone())
        })
        .boxed()
    }
}

// A follower node that receives blocks from the leader node.
// This is used as a state machine (SUT) that we want to test.
struct FollowerNode {
    cryptarchia: Cryptarchia,
}

impl StateMachineTest for FollowerNode {
    // SUT is the real state machine that we want to test.
    type SystemUnderTest = Self;
    // A reference state machine that should be compared against the SUT.
    type Reference = LeaderNode;

    // Initialize the SUT
    fn init_test(
        _ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) -> Self::SystemUnderTest {
        let (cryptarchia, _) = init();
        Self { cryptarchia }
    }

    // Apply the transition on the SUT state and check post-conditions
    fn apply(
        state: Self::SystemUnderTest,
        _ref_state: &<Self::Reference as ReferenceStateMachine>::State,
        transition: <Self::Reference as ReferenceStateMachine>::Transition,
    ) -> Self::SystemUnderTest {
        match transition {
            Transition::ProcessBlock(header) => match state.cryptarchia.try_apply_header(&header) {
                Ok(cryptarchia) => Self { cryptarchia },
                Err(Error::Ledger(cryptarchia_ledger::LedgerError::ParentNotFound(_parent)))
                | Err(Error::Consensus(cryptarchia_engine::Error::ParentMissing(_parent))) => {
                    // TODO: Request the parent block. This is also not implemented yet in the main code.
                    // Because all blocks are proposed by a single leader that extends the chain
                    // without any forks, the follower node cannot extend the chain forever without
                    // the parent block.
                    state
                }
                Err(e) => {
                    panic!("unexpected invalid block: {:?}", e);
                }
            },
        }
    }

    fn check_invariants(
        _state: &Self::SystemUnderTest,
        _ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) {
        // TODO: Check if the SUT is following the reference state (leader's chain).
    }
}

// Initialize Cryptarchia (engine and ledger) and Leader
fn init() -> (Cryptarchia, Leader) {
    let coin = Coin::new(SECRET_KEY, INITIAL_COIN_NONCE.into(), TOTAL_STAKE.into());
    let genesis_state = LedgerState::from_commitments([coin.commitment()], TOTAL_STAKE.into());
    let config = cryptarchia_ledger::Config {
        epoch_stake_distribution_stabilization: 3,
        epoch_period_nonce_buffer: 3,
        epoch_period_nonce_stabilization: 4,
        consensus_config: cryptarchia_engine::Config {
            security_param: 1,
            active_slot_coeff: 1.0,
        },
    };
    (
        Cryptarchia {
            consensus: <cryptarchia_engine::Cryptarchia<_>>::from_genesis(
                GENESIS_ID.into(),
                config.consensus_config.clone(),
            ),
            ledger: <cryptarchia_ledger::Ledger<_>>::from_genesis(
                GENESIS_ID.into(),
                genesis_state,
                config.clone(),
            ),
        },
        Leader::new(GENESIS_ID.into(), vec![coin], config),
    )
}
