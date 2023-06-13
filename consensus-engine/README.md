# Consensus Engine

## Fuzz testing

To test consensus-engine with randomized state transitions,
```bash
cargo test --tests fuzz_test
```

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
If the test starts with the regression files existing, the files are automatically captured and used as test cases to check if the issue is not reproduced anymore.
Thus, these regression files should be added to the Git repository.
For more details, please see the [proptest guide](https://proptest-rs.github.io/proptest/proptest/state-machine.html).

### Test cases

Currently, the fuzz testing generates the following transitions considered as valid.
- `ReceiveBlock`
- `ApproveBlock`

In other words, it doesn't run transitions that are expected to be explicitly rejected by consensus-engine, such as approving blocks that are not received yet.
This means that we test whether the consensus-engine works well if only valid inputs are received.

TODO:
- Test whether the consensus isn't broken if invalid transitions are received.
- Test more transitions for unhappy path.
