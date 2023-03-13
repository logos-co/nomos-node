# Carnot Specification
This is the pseudocode specification of the Carnot consensus algorithm.
In this specification we will omit any cryptographic material, block validity and proof checks. A real implementation is expected to check those before hitting this code.
In addition, all types can be expected to have their invariants checked by the type contract, e.g. in an instance of type `Qc::Aggregate` the `high_qc` field is always the most recent qc among the aggregate qcs and the code can skip this check.

'Q:' is used to indicate unresolved questions.
Notation is loosely based on CDDL.

## Messages
A critical piece in the protocol, these are the different kind of messages used by participants during the protocol execution.
* `Block`: propose a new block
* `Vote`: vote for a block proposal
* `NewView`: propose to jump to a new view after a proposal for the current one was not received before a configurable timeout.


### Block
(sometimes also called proposal)
```
view: View
qc: Qc
``` 
We assume an unique identifier of the block can be obtained, for example by hashing its contents. We will use the `id()` function to access the identifier of the current block.
We also assume that a unique tree order of blocks can be determined, and in particular each participant can identify the parent of each block. We will use the `parent()` function to access such parent block.
        
##### View
```
view_n: uint
```
a monotonically increasing number (considerations about the size?)

##### Qc
There are currently two different types of QC:
```
Qc = Standard / Aggregate
```

###### Standard
```
view: View
block: Id
```
Q: there can only be one block on which consensus in achieved for a view, so maybe the block field is redundant?

###### Aggregate
```
view: View
high_qc: Qc
```
`high_qc` is `Qc` for the most recent view among the aggregated ones. The rest of the qcs are ignored in the rest of this algorithm. 

We assume there is a  `block` function available that returns the block for the Qc. In case of a standard qc, this is trivially qc.block, while for aggregate it can be obtained by accessing `high_qc`. `high_qc` is guaranteed to be a 'Standard' qc.


##### Id
undef, will assume a 32-byte opaque string

### Vote
A vote for `block` in `view`
```      
block: Id
view: View 
voter: Id
? qc: Qc
```
qc is the optional field containing the QC built by root nodes from 2/3 + 1 votes from their child committees and forwarded the the next view leader.

### NewView
```
view: View
high_qc: Qc
```

## Local Variables
Participants in the protocol are expected to mainting the following data in addition to the DAG of received proposal:
* `current_view`
* `local_high_qc`
* `latest_committed_view`
* `collection`: TODO rename


## Available Functions
The following functions are expected to be available to participants during the execution of the protocol:
* `leader(view)`: returns the leader of the view.
* `reset_timer()`: resets timer. If the timer expires the `timeout` routine is triggered.
* `extends(block, ancestor)`: returns true if block is descendant of the ancestor in the chain.

* `download(view)`: Download missing block for the view.
     getMaxViewQC(qcs): returns the qc with the highest view number.
* `member_of_leaf_committee()`: returns true if the participant executing the function is in the leaf committee of the committee overlay.

* `member_of_root_com()`: returns true if the participant executing the function is member of the root committee withing the tree overlay.

* `member_of_internal_com()`: returns truee if the participant executing the function is member of internal committees within the committee tree overlay

* `child_committee(participant)`: returns true if the participant passed as argument is member of the child committee of the participant executing the function.

* `supermajority(votes)`: the behavior changes with the position of a participant in the overlay:
    * Root committee: returns if the number of distinctive signers of votes for a block in the child committee is equal to the threshold.

* `leader_supermajority(votes)`: returns if the number of distinct voters for a block is 2/3 + 1 for both children committees of root committee and overall 2/3 + 1

* `morethanSsupermajority(votes)`: returns if the number of distinctive signers of votes for a block is is more than the threshold: TODO
* `parent_committe`: return the parent committee of the participant executing the function withing the committee tree overlay. Result is undefined if called from a participant in the root committee.


<!-- #####Supermajority of child votes is 2/3 +1 votes from members of child committees
#####Supermajority for the qc to be included in the block, should have at least 2/3+1 votes from both children of the root committee and overal 2/3 +1
#####combined votes of the root committee+its child committees. -->



## Core functions
These are the core functions necessary for the Carnot consensus protocol, to be executed in response of incoming messages, except for `timeout` which is triggered by a participant configurable timer.     
     
### Receive block
```Ruby
Func receive_block(block):
    if block.id() is known OR block.view <= latest_committed_block {
        return
    }

    # Recursively make sure that we process blocks in order
    if block.parent() is missing { 
        let parent = download(block.parent)
        receive(parent)
    }

    if safe_block(block){
        # This is not in the original spec, but 
        # let's validate I have this clear.
        assert block.view = current_view

        update_higQC(block.qc)

        if member_of_leaf_committee() {
            let vote = create_vote()
            if member_of_root_committee() {
                send(vote, leader(current_view + 1))
            } else {
                send(vote, parent_committee())
            }

            current_view = current_view + 1
            reset_timer()
            try_to_commit_grandparent(block)
        }
    }
}
```
##### Auxiliary functions
```Ruby
Func safeBlock(block){
    match block.qc {
        Standard => {
            # Previous leader did not fail and its proposal was certified
            if block.qc.view <= latest_committed_view {
                return false;
            }

            # this check makes sure block is not old 
            # and the previous leader did not fail
            return block.view >= current_view
                AND block.view == block.qc.view + 1
        }

        Aggregate => {
            # Verification of block.aggQC.highQC along 
            # with signature or block.aggQC.signature is sufficient.
            # No need to verify each qc inside block.aggQC
            if block.qc.high_qc.view <= latest_committed_view {
                return false;
            }

            return block.view >= curView
            # we ensure by construction this extends the block in
            # high_qc since that is by definition the parent of this block
        }
    }

}
```
```Ruby
# Commit a grand parent if the grandparent and 
# the parent have been added during two consecutive views.
Func try_to_commit_grandparent(block){ 
    parent = block.parent()
    grandparent = parent.parent()
    return parent.view = grandparent.view+1
        AND block.qc is Standard # Q: Is this necessary?
        AND parent.qc is Standard # Q: Is this necessary?
    # Update last_committed_view
}
```

```Ruby
# Update the latest certification (qc)
Func update_high_qc(qc){
    match qc {
        # Happy case
        Standard => {
            # TODO: revise
            if qc.view > local_high_qc.view{
                local_high_qc = qc
            }

            # Q: The original pseudocde checked for possilbly
            # missing view and downloaded them, but I think
            # we already dealt with this in receive_block
        }
        # Unhappy case
        Agregate => {
            if qc.high_qc.view != local_high_qc.view {
                local_high_qc = qc.high_qc
                # Q: same thing about missing views
            }
        }
    }
}
```

### Receive Vote
Q: this whole function needs to be revised
```Ruby

Func receive_vote(vote){ 
    if vote.block is missing { 
        let block = download(vote.block)
        receive(block)
    }

    # Q: we should probably return if we already received this vote

    if member_of_internal_com() AND not member_of_root() {
        if childcommittee (vote.voter) {
            collection[vote.block].append(vote)
        } else {
            # Q: not returning here would mean it's extremely easy to
            # trigger building a new vote in the following branches
            return;
        }

        if supermajority(collection[vote.block]){
            # Q: should we send it to everyone in the committe?
            let self_vote = build_vote();
            send(self_vote, parentCommittee)
            # Q: why here?
            current_view += 1;
            reset_timer()
            # Q: why do we do this here? 
            try_to_commit_grandparent(block)
        }
    }

    if member_of_root(){
        if childcommittee(vote.voter) { 
            collection[vote.block].append(vote)
        } else {
            # Q: not returning here would mean it's extremely easy to
            # trigger building a new vote in the following branches
            return;
        }

        # Q: Same consideration as above
        if supermajority(collection[vote.block]){
            # Q: The vote to send is not the one received but
            # the one build by this participant, right?
            let self_vote = build_vote();
            qc = build_qc(collection[vote.block])
            self_vote.qc=qc
            send(self_vote, leader(current_view + 1))
            # Q: why here?
            current_view += 1
            reset_timer()
            # Q: why here?
            try_to_commit_grandparent(block)
        }

        # Q: this means that we send a message for every incoming
        # message after the threshold has been reached, i.e. a vote
        # from a node in the leaf committee can trigger
        # at least height(tree) messages.
        if morethanSsupermajority(collection[vote.block]) {
            # just forward the vote to the leader
            # Q: But then childcommitte(vote.voter) would return false
            # in the leader, as it's a granchild, not a child
            send(vote, leader(current_view + 1))
        }
    }

    if self = leader(view) {
        if vote.view < current_view - 1 { 
            return
        }

        # Q: No filtering? I can just create a key and  vote?
        collection[vote.block].append(vote)
        if supermajority(collection[vote.block]){
            qc = build_qc(collection[vote.block])
            block = build_block(qc)
            broadcast(block)
        }
    }
}
```

### Receive NewView
```Ruby
# Failure Case
Func receive(newView) {
    # download the missing block 
    if newview.highQC.block missing {
        let block = download(new_view.high_qc.block)
        receive(block)
    }

    # It's an old message. Ignore it.
    if newView.view < current_view {
        return 
    }

    # Q: this was update_high_qc_and_view(new_view.high_qc, Null)
    update_high_qc(new_view.high_qc)

    if member_of_internal_com() {
        collection[newView.view].append(newView)
        if supermajority[newView.view]{
            newViewQC=buildQC(collection[newView.view])
            if member_of_root(){
                send(newViewQC, leader(view+1))
                curView++
            } else {
                send(newViewQC, parent_committee())
            }
        }
    }
}
```
### Timeout
```Ruby
Func timeout(){
    unimplemented
}
```     


We need to make sure that qcs can't be removed from aggQc when going up the tree