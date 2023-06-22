mod fuzz;

use proptest::prelude::*;
use proptest::test_runner::Config;
use proptest_state_machine::{prop_state_machine, ReferenceStateMachine, StateMachineTest};

use fuzz::sut::ConsensusEngineTest;

prop_state_machine! {
    #![proptest_config(Config {
        // Only run 100 cases by default to avoid running out of system resources
        // and taking too long to finish.
        cases: 100,
        .. Config::default()
    })]

    #[test]
    // run 50 state transitions per test case
    fn consensus_engine_test(sequential 1..50 => ConsensusEngineTest);
}
