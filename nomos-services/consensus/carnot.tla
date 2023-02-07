---- MODULE carnot ----
EXTENDS TLC, Integers, Sequences, FiniteSets

NODES == 5
MAX_VIEW == 10

(*--algorithm carnot
variables
    \* each node should have its own individual view
    view = [i \in 0..NODES |-> 0];
    votes = [_p \in 0..MAX_VIEW |-> 0];
    blocks = [i \in 1..NODES |-> <<>>];

define
    \* in the end, assuming nodes do not go offline or block indefinitely,
    \* they all reach the last view
    SharedEnd ==
        <> \A i \in 0..NODES: view[i] = MAX_VIEW
    \* at any moment, at least half of the nodes should agree on the view
    \* MajoritySharedMiddle ==
    \*     [](\E v \in 0..MAX_VIEW:  Cardinality({i \in DOMAIN view: view[i] = v}) * 2 >= NODES)
    \* leader can't jump more than 1 view at a times
    LeaderNoRun ==
        [](Cardinality({i \in DOMAIN view: view[i] = view[0] \/ view[i] = view[0] - 1}) * 2 >= NODES)
    \* No node can have a view bigger than the one of the leader
    LeaderLeading ==
        [](\A i \in DOMAIN view: view[i] <= view[0])
    ViewOnlyIncrease ==
        [][\A i \in DOMAIN view: view[i]' = view[i] + 1 \/ view[i]' = view[i]]_view
end define;

fair process leader = 0
variables
    t = 1;
begin
    Begin:
        votes[0]:= 4;
    WaitForVotes:
        await votes[view[0]] * 2 >= NODES;
    CreateBlock:
        \* maybe more complex logic?
        view[0] := view[0] + 1;
    SendBlock:
        \* no erasure codes, leader send the whole block
        \* to all participants
        while t <= NODES do
            blocks[t] := Append(blocks[t], view[0]);
            t := t + 1;
        end while;
        t := 1;
        if view[0] < MAX_VIEW then
            goto WaitForVotes;
        end if;
    End:
        skip;
end process;


fair process nodes \in 1..NODES
begin
    WaitForBlock:
        await blocks[self] # <<>>;
        \* TODO verify the block is valid, we only do happy path for now
        \* assert blocks[self] = view;
        \* update the view according to the block, nodes only process
        \* one view at a time
        view[self] := Head(blocks[self]);    
        blocks[self] := Tail(blocks[self]);
    SendVote:
        votes[view[self]] := votes[view[self]] + 1;
        if view[self] < MAX_VIEW then
            goto WaitForBlock;
        end if;
    End:
        skip;
end process;
end algorithm; *)
\* BEGIN TRANSLATION (chksum(pcal) = "8c938250" /\ chksum(tla) = "edd986f4")
\* Label End of process leader at line 55 col 9 changed to End_
VARIABLES view, votes, blocks, pc

(* define statement *)
SharedEnd ==
    <> \A i \in 0..NODES: view[i] = MAX_VIEW




LeaderNoRun ==
    [](Cardinality({i \in DOMAIN view: view[i] = view[0] \/ view[i] = view[0] - 1}) * 2 >= NODES)

LeaderLeading ==
    [](\A i \in DOMAIN view: view[i] <= view[0])
ViewOnlyIncrease ==
    [][\A i \in DOMAIN view: view[i]' = view[i] + 1 \/ view[i]' = view[i]]_view

VARIABLE t

vars == << view, votes, blocks, pc, t >>

ProcSet == {0} \cup (1..NODES)

Init == (* Global variables *)
        /\ view = [i \in 0..NODES |-> 0]
        /\ votes = [_p \in 0..MAX_VIEW |-> 0]
        /\ blocks = [i \in 1..NODES |-> <<>>]
        (* Process leader *)
        /\ t = 1
        /\ pc = [self \in ProcSet |-> CASE self = 0 -> "Begin"
                                        [] self \in 1..NODES -> "WaitForBlock"]

Begin == /\ pc[0] = "Begin"
         /\ votes' = [votes EXCEPT ![0] = 4]
         /\ pc' = [pc EXCEPT ![0] = "WaitForVotes"]
         /\ UNCHANGED << view, blocks, t >>

WaitForVotes == /\ pc[0] = "WaitForVotes"
                /\ votes[view[0]] * 2 >= NODES
                /\ pc' = [pc EXCEPT ![0] = "CreateBlock"]
                /\ UNCHANGED << view, votes, blocks, t >>

CreateBlock == /\ pc[0] = "CreateBlock"
               /\ view' = [view EXCEPT ![0] = view[0] + 1]
               /\ pc' = [pc EXCEPT ![0] = "SendBlock"]
               /\ UNCHANGED << votes, blocks, t >>

SendBlock == /\ pc[0] = "SendBlock"
             /\ IF t <= NODES
                   THEN /\ blocks' = [blocks EXCEPT ![t] = Append(blocks[t], view[0])]
                        /\ t' = t + 1
                        /\ pc' = [pc EXCEPT ![0] = "SendBlock"]
                   ELSE /\ t' = 1
                        /\ IF view[0] < MAX_VIEW
                              THEN /\ pc' = [pc EXCEPT ![0] = "WaitForVotes"]
                              ELSE /\ pc' = [pc EXCEPT ![0] = "End_"]
                        /\ UNCHANGED blocks
             /\ UNCHANGED << view, votes >>

End_ == /\ pc[0] = "End_"
        /\ TRUE
        /\ pc' = [pc EXCEPT ![0] = "Done"]
        /\ UNCHANGED << view, votes, blocks, t >>

leader == Begin \/ WaitForVotes \/ CreateBlock \/ SendBlock \/ End_

WaitForBlock(self) == /\ pc[self] = "WaitForBlock"
                      /\ blocks[self] # <<>>
                      /\ view' = [view EXCEPT ![self] = Head(blocks[self])]
                      /\ blocks' = [blocks EXCEPT ![self] = Tail(blocks[self])]
                      /\ pc' = [pc EXCEPT ![self] = "SendVote"]
                      /\ UNCHANGED << votes, t >>

SendVote(self) == /\ pc[self] = "SendVote"
                  /\ votes' = [votes EXCEPT ![view[self]] = votes[view[self]] + 1]
                  /\ IF view[self] < MAX_VIEW
                        THEN /\ pc' = [pc EXCEPT ![self] = "WaitForBlock"]
                        ELSE /\ pc' = [pc EXCEPT ![self] = "End"]
                  /\ UNCHANGED << view, blocks, t >>

End(self) == /\ pc[self] = "End"
             /\ TRUE
             /\ pc' = [pc EXCEPT ![self] = "Done"]
             /\ UNCHANGED << view, votes, blocks, t >>

nodes(self) == WaitForBlock(self) \/ SendVote(self) \/ End(self)

(* Allow infinite stuttering to prevent deadlock on termination. *)
Terminating == /\ \A self \in ProcSet: pc[self] = "Done"
               /\ UNCHANGED vars

Next == leader
           \/ (\E self \in 1..NODES: nodes(self))
           \/ Terminating

Spec == /\ Init /\ [][Next]_vars
        /\ WF_vars(leader)
        /\ \A self \in 1..NODES : WF_vars(nodes(self))

Termination == <>(\A self \in ProcSet: pc[self] = "Done")

\* END TRANSLATION 

====
