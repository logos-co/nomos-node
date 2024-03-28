use std::{collections::HashMap, panic};

use cryptarchia_consensus::Cryptarchia;
use cryptarchia_engine::Slot;
use cryptarchia_ledger::{Coin, Ledger, LedgerState, Value};
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
    fn cryptarchia_ledger_test(sequential 1..100 => CryptarchiaLedgerTest);
}

const GENESIS_ID: [u8; 32] = [0; 32];
const SECRET_KEY: [u8; 32] = [0; 32];
const INITIAL_COIN_NONCE: [u8; 32] = [0; 32];
const TOTAL_STAKE: u32 = 1;

#[derive(Clone, Debug)]
pub enum Transition {
    Update { header: Header, coin: Coin },
}

// A state machine that we want to test
pub struct CryptarchiaLedgerTest {
    cryptarchia: Cryptarchia,
}

impl StateMachineTest for CryptarchiaLedgerTest {
    // SUT is the real state machine that we want to test.
    type SystemUnderTest = Self;
    // A reference state machine that should be compared against the SUT.
    type Reference = RefState;

    // Initialize the SUT
    fn init_test(
        _ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) -> Self::SystemUnderTest {
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
        let cryptarchia = Cryptarchia {
            consensus: <cryptarchia_engine::Cryptarchia<_>>::from_genesis(
                GENESIS_ID.into(),
                config.consensus_config.clone(),
            ),
            ledger: Ledger::from_genesis(GENESIS_ID.into(), genesis_state, config),
        };

        Self { cryptarchia }
    }

    // Apply the transition on the SUT state and check post-conditions
    fn apply(
        state: Self::SystemUnderTest,
        _ref_state: &<Self::Reference as ReferenceStateMachine>::State,
        transition: <Self::Reference as ReferenceStateMachine>::Transition,
    ) -> Self::SystemUnderTest {
        match transition {
            Transition::Update { header, .. } => {
                let cryptarchia = state.cryptarchia.try_apply_header(&header).unwrap();
                //TODO: add post-condition checks
                Self { cryptarchia }
            }
        }
    }

    fn check_invariants(
        state: &Self::SystemUnderTest,
        ref_state: &<Self::Reference as ReferenceStateMachine>::State,
    ) {
        //TODO: add invariant checks
    }
}

// Reference state is used to drive the generation of a sequence of transitions
// and can also be compared against the state of the SUT.
// This should contain the minimum required information needed to mimic the state machine.
#[derive(Clone, Debug)]
pub struct RefState {
    blocks: HashMap<HeaderId, (HeaderId, Slot)>,
    coins: HashMap<HeaderId, Vec<Coin>>,
}

impl ReferenceStateMachine for RefState {
    type State = Self;
    type Transition = Transition;

    // Initialize the reference state machine
    fn init_state() -> BoxedStrategy<Self::State> {
        let mut blocks = HashMap::new();
        blocks.insert(GENESIS_ID.into(), (GENESIS_ID.into(), 0.into()));
        let mut coins = HashMap::new();
        coins.insert(
            GENESIS_ID.into(),
            vec![Coin::new(
                SECRET_KEY,
                INITIAL_COIN_NONCE.into(),
                TOTAL_STAKE.into(),
            )],
        );

        Just(Self { blocks, coins }).boxed()
    }

    // Generate transitions based on the current reference state machine
    fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        state.transition_update()
    }

    // Check if the transition is valid for a given reference state, before applying the transition
    // If invalid, the transition will be ignored and a new transition will be generated.
    //
    // In most cases, this will return true if transitions are always generated
    // correctly by the `transitions` function.
    // However, preconditions also check if the transition is still valid
    // during shrinking in failure cases.
    // If the transition became invalid for the shrunk state,
    // the shrinking is stopped or continued to other directions.
    fn preconditions(state: &Self::State, transition: &Self::Transition) -> bool {
        match transition {
            Transition::Update { header, coin } => {
                state.blocks.contains_key(&header.parent())
                    && state
                        .coins
                        .get(&header.parent())
                        .map_or(false, |coins| coins.contains(coin))
            }
        }
    }

    // Apply the given transition on the reference state machine,
    // so that it can be used to generate next transitions.
    fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
        match transition {
            Transition::Update { header, coin } => {
                state
                    .blocks
                    .insert(header.id(), (header.parent(), header.slot()));
                state
                    .coins
                    .entry(header.id())
                    .or_insert(vec![])
                    .push(coin.evolve());
                state
            }
        }
    }
}

impl RefState {
    fn transition_update(&self) -> BoxedStrategy<Transition> {
        // Choose one of any existing blocks (including genesis) as parent
        let blocks = self.blocks.clone();
        let coins = self.coins.clone();
        proptest::sample::select(self.blocks.keys().cloned().collect::<Vec<_>>())
            .prop_flat_map(move |parent| {
                let parent_slot = blocks[&parent].1;
                let coin = *coins.get(&parent).unwrap().last().unwrap();
                (1..10u64).prop_map(move |slot_jump| {
                    let slot = parent_slot + slot_jump;
                    Transition::Update {
                        header: Builder::new(parent, coin.to_proof(slot))
                            // .with_orphaned_proofs(todo!())
                            .build([0; 32].into(), 0),
                        coin,
                    }
                })
            })
            .boxed()
    }
}
