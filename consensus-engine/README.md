# Consensus Engine

## Fuzz testing

### Running fuzz tests

To test consensus-engine with randomized state transitions,
```bash
cargo test --test fuzz_test
```

By default, the fuzz test runs only [few test cases and transisions](tests/fuzz_test.rs#L15).
To trigger a long-running fuzz test, please use the following environment variables:
```bash
PROPTEST_CASES=500000 \  # Num of successful test cases that must execute
cargo test --test fuzz_test
```
The [hardcoded number](tests/fuzz_test.rs#L20) of state transitions will be executed for each test case.
This number needs to be configurable soon.

If you want to print transitions,
```bash
# 0: No extra output
# 1: Log test failure messages
# 2: Trace low-level details
PROPTEST_CASES=2 \
cargo test --test fuzz_test -- --nocapture
```

For more details about `PROPTEST_*` environment variables, please see the [proptest guide](https://github.com/proptest-rs/proptest/blob/7d840ca5071bed1a986dd7e0db080847a07c9818/proptest/src/test_runner/config.rs#L186).

### Regression files

If the fuzz testing finds any failure case, it will generate a regression file: `tests/fuzz_test.proptest-regressions` that contains the initial state and transitions that cause the failure. The file looks like below.
```
# Seeds for failure cases proptest has generated in the past. It is
# automatically read and these particular cases re-run before any
# novel cases are generated.
#
# It is recommended to check this file in to source control so that
# everyone who runs the test benefits from these saved cases.
cc c2157c559fe10276985a8f2284b0c294c2d6a5a293cce45f2e4ad2a3b4a23233 # shrinks to (initial_state, transitions) = (RefState { chain: {0: {[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]: Block { id: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], view: 0, parent_qc: Standard(StandardQc { view: -1, id: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] }) }}}, blocks: {[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]: Block { id: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], view: 0, parent_qc: Standard(StandardQc { view: -1, id: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] }) }}, highest_voted_view: 0 }, [ReceiveBlock([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])])
```
We should manually check if the error is caused by a break in consensus-engine, or by something wrong in the fuzz test.

If the error came from consensus-engine, we should fix the consensus-engine and commit the fix to Git along with the regression file.
If the fuzz test starts with the regression files existing, the files are automatically captured and used as test cases to check if the issue is not reproduced anymore.

If the error came from something wrong in the fuzz test, we should fix the test.
In this case, we don't need to (or shouldn't) push the regression file to Git.

For more details, please see the [proptest guide](https://proptest-rs.github.io/proptest/proptest/state-machine.html).

_NOTE: It seems that the regression file isn't loaded by [proptest-state-machine-testing](https://proptest-rs.github.io/proptest/proptest/state-machine.html). @youngjoon-lee should find the right way._
