---- MODULE carnot ----
EXTENDS TLC, Integers, Sequences, FiniteSets

NODES == 4
MAX_VIEW == 3

(*--algorithm carnot_flat
variables
    \* Almost all of these variables could be node-local, but we can't write properties
    \* involving process local variables, so we make them global

    \* each node should have its own individual view
    view = [_i \in 1..NODES |-> 0];
    \* number of votes for each view
    votes = [_i \in 0..MAX_VIEW |-> 0];
    \* sorted queue for each node containing incoming blocks
    blocks = [_i \in 1..NODES |-> <<>>];
    \* the leader for each view according to each node
    leaders = [_i \in 1..NODES, _j \in 0..MAX_VIEW |-> -1];
    \* conventient variable to store number of leaders for each view
    view_leaders = [_i \in 0..MAX_VIEW |-> 0];
    \* some magic oracle that determined the leader for each view
    leader_sel \in [ 0..MAX_VIEW -> 1..NODES];

define
    \* in the end, assuming nodes do not go offline or block indefinitely,
    \* they all reach the last view
    SharedEnd ==
        <> \A i \in 1..NODES: view[i] = MAX_VIEW
    \* leader can't be too far ahead of the other nodes, also, at any given moment
    \* at least half of the nodes should agree on the view
    LeaderNoRun ==
        []~(\E i \in 1..NODES: \A j \in 1..NODES: view[i] > view[j] + 1 \/ i = j)
    \* a view can only increase by 1
    ViewOnlyIncreaseByOne ==
        [][\A i \in DOMAIN view: view[i]' = view[i] + 1 \/ view[i]' = view[i]]_view
    OnlyOneLeaderPerView ==
        [](\A i \in DOMAIN view: view_leaders[view[i]] <= 1)
    NodesAgreeOnLeader ==
        \* it's eventually true that for each view, it exists a node which is recognized as leader
        \* by all other nodes
        \* it's 'eventually' because nodes might lag behind
        []<>(\A v \in 0..MAX_VIEW: \E _leader \in 1..NODES: \A n \in 1..NODES: leaders[n, v] = _leader)

    Sort(seq) ==
        SortSeq(seq, LAMBDA x, y: x.v < y.v)
end define;

macro update_leader_and_end(next_leader) begin
    leaders[self, view[self]] := next_leader;
    if view[self] < MAX_VIEW then
        goto Begin;
    else
        goto End;
    end if;
end macro;

fair process node \in 1..NODES
variables
    _node = 1;
    next_leader = 0;
begin
    Start:
        votes[0] := NODES;
        leaders[self, 0] := 1;
    Begin:
        assert leaders[self, view[self]] # -1;
        if leaders[self, view[self]] = self then
            goto Leader;
        else
            goto Follower;
        end if;


    \* ---------- FOLLOWER
    Follower:
        \* ----- Wait for a block to be received
        await blocks[self] # <<>> /\ Head(blocks[self]).v = view[self] + 1;
        \* TODO verify the block is valid, we only do happy path for now
        \* in the current implementation we only process one view at a time
        \* which is equivalent to saying that we don't jump views
        assert Head(blocks[self]).v = view[self] + 1;
        \* generate next view and leader
        view[self] := Head(blocks[self]).v;
        next_leader := Head(blocks[self]).msg;
        \* remove processed block from the queue
        blocks[self] := Tail(blocks[self]);
        \* ----- Send a vote to the leader
        votes[view[self]] := votes[view[self]] + 1;
        \* compute next leader, make it depend on the contents of the block
        update_leader_and_end(next_leader);

    \* ---------- LEADER
    Leader:
        view_leaders[view[self]] := view_leaders[view[self]] + 1;
        \* ----- Wait for enough votes
        await votes[view[self]] * 2 >= NODES;
        \* ----- Generate block for next
        \* maybe more complex logic?
        view[self] := view[self] + 1;
        \* while we devise how to determine next leadr, let's deterministically
        \* derive it from current block. ti = next leader
        next_leader := leader_sel[view[self]];
    SendBlock:
        \* no erasure codes, leader send the whole block
        \* to all participants, assuming reliable dissemination
        while _node <= NODES do
            if _node # self then
                blocks[_node] := Sort(Append(blocks[_node], [v |-> view[self], msg |-> next_leader ]));
            end if;
            _node := _node + 1;
        end while;
        _node := 1;
        update_leader_and_end(next_leader);
    End:
        skip;
end process;
end algorithm; *)
\* BEGIN TRANSLATION (chksum(pcal) = "86c4f1a3" /\ chksum(tla) = "5baebc75")
VARIABLES view, votes, blocks, leaders, view_leaders, leader_sel, pc

(* define statement *)
SharedEnd ==
    <> \A i \in 1..NODES: view[i] = MAX_VIEW


LeaderNoRun ==
    []~(\E i \in 1..NODES: \A j \in 1..NODES: view[i] > view[j] + 1 \/ i = j)

ViewOnlyIncreaseByOne ==
    [][\A i \in DOMAIN view: view[i]' = view[i] + 1 \/ view[i]' = view[i]]_view
OnlyOneLeaderPerView ==
    [](\A i \in DOMAIN view: view_leaders[view[i]] <= 1)
NodesAgreeOnLeader ==



    []<>(\A v \in 0..MAX_VIEW: \E _leader \in 1..NODES: \A n \in 1..NODES: leaders[n, v] = _leader)

Sort(seq) ==
    SortSeq(seq, LAMBDA x, y: x.v < y.v)

VARIABLES _node, next_leader

vars == << view, votes, blocks, leaders, view_leaders, leader_sel, pc, _node, 
           next_leader >>

ProcSet == (1..NODES)

Init == (* Global variables *)
        /\ view = [_i \in 1..NODES |-> 0]
        /\ votes = [_i \in 0..MAX_VIEW |-> 0]
        /\ blocks = [_i \in 1..NODES |-> <<>>]
        /\ leaders = [_i \in 1..NODES, _j \in 0..MAX_VIEW |-> -1]
        /\ view_leaders = [_i \in 0..MAX_VIEW |-> 0]
        /\ leader_sel \in [ 0..MAX_VIEW -> 1..NODES]
        (* Process node *)
        /\ _node = [self \in 1..NODES |-> 1]
        /\ next_leader = [self \in 1..NODES |-> 0]
        /\ pc = [self \in ProcSet |-> "Start"]

Start(self) == /\ pc[self] = "Start"
               /\ votes' = [votes EXCEPT ![0] = NODES]
               /\ leaders' = [leaders EXCEPT ![self, 0] = 1]
               /\ pc' = [pc EXCEPT ![self] = "Begin"]
               /\ UNCHANGED << view, blocks, view_leaders, leader_sel, _node, 
                               next_leader >>

Begin(self) == /\ pc[self] = "Begin"
               /\ Assert(leaders[self, view[self]] # -1, 
                         "Failure of assertion at line 67, column 9.")
               /\ IF leaders[self, view[self]] = self
                     THEN /\ pc' = [pc EXCEPT ![self] = "Leader"]
                     ELSE /\ pc' = [pc EXCEPT ![self] = "Follower"]
               /\ UNCHANGED << view, votes, blocks, leaders, view_leaders, 
                               leader_sel, _node, next_leader >>

Follower(self) == /\ pc[self] = "Follower"
                  /\ blocks[self] # <<>> /\ Head(blocks[self]).v = view[self] + 1
                  /\ Assert(Head(blocks[self]).v = view[self] + 1, 
                            "Failure of assertion at line 82, column 9.")
                  /\ view' = [view EXCEPT ![self] = Head(blocks[self]).v]
                  /\ next_leader' = [next_leader EXCEPT ![self] = Head(blocks[self]).msg]
                  /\ blocks' = [blocks EXCEPT ![self] = Tail(blocks[self])]
                  /\ votes' = [votes EXCEPT ![view'[self]] = votes[view'[self]] + 1]
                  /\ leaders' = [leaders EXCEPT ![self, view'[self]] = next_leader'[self]]
                  /\ IF view'[self] < MAX_VIEW
                        THEN /\ pc' = [pc EXCEPT ![self] = "Begin"]
                        ELSE /\ pc' = [pc EXCEPT ![self] = "End"]
                  /\ UNCHANGED << view_leaders, leader_sel, _node >>

Leader(self) == /\ pc[self] = "Leader"
                /\ view_leaders' = [view_leaders EXCEPT ![view[self]] = view_leaders[view[self]] + 1]
                /\ votes[view[self]] * 2 >= NODES
                /\ view' = [view EXCEPT ![self] = view[self] + 1]
                /\ next_leader' = [next_leader EXCEPT ![self] = leader_sel[view'[self]]]
                /\ pc' = [pc EXCEPT ![self] = "SendBlock"]
                /\ UNCHANGED << votes, blocks, leaders, leader_sel, _node >>

SendBlock(self) == /\ pc[self] = "SendBlock"
                   /\ IF _node[self] <= NODES
                         THEN /\ IF _node[self] # self
                                    THEN /\ blocks' = [blocks EXCEPT ![_node[self]] = Sort(Append(blocks[_node[self]], [v |-> view[self], msg |-> next_leader[self] ]))]
                                    ELSE /\ TRUE
                                         /\ UNCHANGED blocks
                              /\ _node' = [_node EXCEPT ![self] = _node[self] + 1]
                              /\ pc' = [pc EXCEPT ![self] = "SendBlock"]
                              /\ UNCHANGED leaders
                         ELSE /\ _node' = [_node EXCEPT ![self] = 1]
                              /\ leaders' = [leaders EXCEPT ![self, view[self]] = next_leader[self]]
                              /\ IF view[self] < MAX_VIEW
                                    THEN /\ pc' = [pc EXCEPT ![self] = "Begin"]
                                    ELSE /\ pc' = [pc EXCEPT ![self] = "End"]
                              /\ UNCHANGED blocks
                   /\ UNCHANGED << view, votes, view_leaders, leader_sel, 
                                   next_leader >>

End(self) == /\ pc[self] = "End"
             /\ TRUE
             /\ pc' = [pc EXCEPT ![self] = "Done"]
             /\ UNCHANGED << view, votes, blocks, leaders, view_leaders, 
                             leader_sel, _node, next_leader >>

node(self) == Start(self) \/ Begin(self) \/ Follower(self) \/ Leader(self)
                 \/ SendBlock(self) \/ End(self)

(* Allow infinite stuttering to prevent deadlock on termination. *)
Terminating == /\ \A self \in ProcSet: pc[self] = "Done"
               /\ UNCHANGED vars

Next == (\E self \in 1..NODES: node(self))
           \/ Terminating

Spec == /\ Init /\ [][Next]_vars
        /\ \A self \in 1..NODES : WF_vars(node(self))

Termination == <>(\A self \in ProcSet: pc[self] = "Done")

\* END TRANSLATION 

====
