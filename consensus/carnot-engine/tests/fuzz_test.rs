mod fuzz;

use std::panic;

use fuzz::sut::ConsensusEngineTest;
use proptest::{prelude::*, test_runner::Config};
use proptest_state_machine::{prop_state_machine, ReferenceStateMachine, StateMachineTest};

prop_state_machine! {
    #![proptest_config(Config {
        // Only run 10 cases by default to avoid running out of system resources
        // and taking too long to finish.
        cases: 10,
        .. Config::default()
    })]

    #[test]
    // run 100 state transitions per test case
    fn consensus_engine_test(sequential 1..30 => ConsensusEngineTest);
}
