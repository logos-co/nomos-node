---- MODULE carnot_flat ----
EXTENDS TLC, Integers, Sequences, FiniteSets

CONSTANTS Quorum, Node, MaxView

\* Constrain Block to some bounded value
Block == Node

(***************************************************************************)
(* What follows next is a PlusCal specification of a flat Carnot           *)
(* implementation. We will then prove that this implementation is a valid  *)
(* implementation of the Carnot consensus algorithm.                       *)
(*                                                                         *)
(* PlusCal is form of TLA+ that looks like pseudocode and should be easier *)
(* to follow. It has to be transformed into TLA+ before it can be checked  *)
(* so remember to do it each time you modify something. Everything after   *)
(* 'BEGIN_TRANSLATION' is the automatic translation of PlusCal.            *)
(***************************************************************************)

(*--algorithm carnot_flat
variables
    \* Almost all of these variables could be node-local, but we can't write
    \* properties involving process local variables, so we make them global
    
    \* Most recent view for each node
    view = [n \in Node |-> -1];
    
    \* The leader for each view according to each node
    \* This may be used to check assumptions on its value
    leaders = [n \in Node, v \in 0..MaxView |-> -1];

    \* conventient variable to store number of leaders for each view
    view_leaders = [v \in 0..MaxView 
        |-> Cardinality({l \in Node: \E n \in Node: leaders[n, v] = l})];

    \* magic oracle to determine the leader for each view
    leader \in [0..MaxView+1 -> Node];

    \* Set of messages exchanged by parties. Assumes global view of the network
    msgs = {};

define
    \* Here we can have 'local' predicates for the algorithm that are useful
    \* to check assumptions on the specific implementation
    TypeOK == /\ view \in [Node -> 0..MaxView \cup {-1}]
              /\ leaders \in [(Node \X (0..MaxView)) -> Node \cup {-1}]
              /\ view_leaders \in [0..MaxView -> Nat]
              /\ leader \in [0..MaxView+1 -> Node]
              /\ msgs \subseteq [type: {"block", "vote"}, node: Node \cup {-1}, view: 0..MaxView, block: Block]
            \* field 'block' both refers to block indentifier in a vote and
            \* block contents in a proposal

    \* leader can't be too far ahead of the other nodes
    LeaderNoRun ==
        ~(\E n1 \in Node: \A n2 \in Node \ {n1}: view[n1] > view[n2] + 1)

    Inv == /\ TypeOK
           /\ LeaderNoRun

    \* Check that an implementation processing views one by one is correct
    ViewOnlyIncreaseByOne ==
        [][\A n \in Node: \/ view[n]' = view[n] + 1 
                          \/ view[n]' = view[n]
          ]_view
    
    \* Honest nodes agree on the leader for a view
    \* WARNING: this is true only eventually, as nodes are allowed to lag 
    \*          behind and not process views in sync
    NodesAgreeOnLeader(v) ==
        \E l \in Node: \A n \in Node: leaders[n, v] = l

    EventualAgreement ==
        \A v \in 0..MaxView: <>NodesAgreeOnLeader(v)

    \* Equivalent to ChosenAt(v, b)
    BlockApproved(v, b) == 
        \E q \in Quorum: 
            \A node \in q: 
                \E m \in msgs: /\ m.type  = "vote" 
                               /\ m.view  = v
                               /\ m.block = b 
                               /\ m.node  = node    

    \* The following expressions link this implementation to the Carnot specification. 
    \* Their values will be used to check that this algorithm implements Carnot.
    votes == 
        [n \in Node |->  
            {<<m.view, m.block>>: 
                m \in {mm \in msgs: mm.type = "vote" /\ mm.node = n }}]

    proposals == 
        [n \in Node |->  
            {<<m.view, m.block>>:
                m \in {mm \in msgs: mm.type = "block" /\ mm.node = n }}]

    \* This imports the Carnot specification from the 'carnot' modules.
    \* It will implicity use the 'votes' and 'proposals' expressions above as
    \* the 'votes' and 'proposals' variables in the Carnot specification.
    INSTANCE carnot
            
end define;


\* Convenience macros

\* Update the value of 'leaders' and jump to the beginnig.
macro update_leader_and_end(next_leader) begin
    leaders[self, view[self]] := next_leader;
    if view[self] < MaxView then
        goto Begin;
    else
        goto End;
    end if;
end macro;

\* 'fair' means that process do not crash. It's needed for checking liveness properties.
fair process node \in Node
variables
    _msg = [type |-> "", view |-> -1, block |-> -1, node |-> -1];
    next_leader = 0;
begin
    Start:
        \* Genesis block is special, nobody is a leader
        msgs := msgs \cup {[type |-> "block", view |-> 0, block |-> leader[1], node |-> -1]};
        goto FollowerNode;
    Begin:
        assert leaders[self, view[self]] # -1;
        if leaders[self, view[self]] = self then
            goto LeaderNode;
        else
            goto FollowerNode;
        end if;


    \* ---------- FOLLOWER
    FollowerNode:
        \* ----- Wait for a block proposal to be received
        await \E m \in msgs: (m.type = "block" /\ m.view = view[self] + 1);
        \* ----- Get block from the queue but do not remove it
        _msg := CHOOSE m \in msgs: (m.type = "block" /\ m.view = view[self] + 1);
        view[self] := _msg.view;
        \* ----- Assume the next leader can be determined from current block contents
        next_leader := _msg.block;
        \* ----- Vote for next block
        msgs := msgs \cup {[type |-> "vote", view |-> view[self], block |-> _msg.block , node |-> self]};
        \* compute next leader, make it depend on the contents of the block
        update_leader_and_end(next_leader);

    \* ---------- LEADER
    LeaderNode:
        \* ------ We are leader for the next view, so create the new block
        \*        and advance the view
        view[self] := view[self] + 1;
        view_leaders[view[self]] := view_leaders[view[self]] + 1;
        \* ----- Wait for past block to be approved by a quorum
        await \E b \in Block: BlockApproved(view[self] - 1, b);
        \* ----- Generate block for next
        
        next_leader := leader[view[self] + 1];
        \* Suppose the magic oracle for view v is only available to the leader
        \* of view v - 1. I.e. nodes can only peek one view in the future.
        \* In pratice, this means that the leader can be determined deterministically
        \* from the contents of the block.
        msgs := msgs \cup  {[type |-> "block", view |-> view[self], block |-> next_leader, node |-> self]};
        update_leader_and_end(next_leader);
    End:
        skip;
end process;
end algorithm; *)
\* BEGIN TRANSLATION (chksum(pcal) = "f248c9ac" /\ chksum(tla) = "ec0c7c9")
VARIABLES view, leaders, view_leaders, leader, msgs, pc

(* define statement *)
TypeOK == /\ view \in [Node -> 0..MaxView \cup {-1}]
          /\ leaders \in [(Node \X (0..MaxView)) -> Node \cup {-1}]
          /\ view_leaders \in [0..MaxView -> Nat]
          /\ leader \in [0..MaxView+1 -> Node]
          /\ msgs \subseteq [type: {"block", "vote"}, node: Node \cup {-1}, view: 0..MaxView, block: Block]




LeaderNoRun ==
    ~(\E n1 \in Node: \A n2 \in Node \ {n1}: view[n1] > view[n2] + 1)

Inv == /\ TypeOK
       /\ LeaderNoRun


ViewOnlyIncreaseByOne ==
    [][\A n \in Node: \/ view[n]' = view[n] + 1
                      \/ view[n]' = view[n]
      ]_view




NodesAgreeOnLeader(v) ==
    \E l \in Node: \A n \in Node: leaders[n, v] = l

EventualAgreement ==
    \A v \in 0..MaxView: <>NodesAgreeOnLeader(v)


BlockApproved(v, b) ==
    \E q \in Quorum:
        \A node \in q:
            \E m \in msgs: /\ m.type  = "vote"
                           /\ m.view  = v
                           /\ m.block = b
                           /\ m.node  = node



votes ==
    [n \in Node |->
        {<<m.view, m.block>>:
            m \in {mm \in msgs: mm.type = "vote" /\ mm.node = n }}]

proposals ==
    [n \in Node |->
        {<<m.view, m.block>>:
            m \in {mm \in msgs: mm.type = "block" /\ mm.node = n }}]




INSTANCE carnot

VARIABLES _msg, next_leader

vars == << view, leaders, view_leaders, leader, msgs, pc, _msg, next_leader
        >>

ProcSet == (Node)

Init == (* Global variables *)
        /\ view = [n \in Node |-> -1]
        /\ leaders = [n \in Node, v \in 0..MaxView |-> -1]
        /\ view_leaders =            [v \in 0..MaxView
                          |-> Cardinality({l \in Node: \E n \in Node: leaders[n, v] = l})]
        /\ leader \in [0..MaxView+1 -> Node]
        /\ msgs = {}
        (* Process node *)
        /\ _msg = [self \in Node |-> [type |-> "", view |-> -1, block |-> -1, node |-> -1]]
        /\ next_leader = [self \in Node |-> 0]
        /\ pc = [self \in ProcSet |-> "Start"]

Start(self) == /\ pc[self] = "Start"
               /\ msgs' = (msgs \cup {[type |-> "block", view |-> 0, block |-> leader[1], node |-> -1]})
               /\ pc' = [pc EXCEPT ![self] = "FollowerNode"]
               /\ UNCHANGED << view, leaders, view_leaders, leader, _msg, 
                               next_leader >>

Begin(self) == /\ pc[self] = "Begin"
               /\ Assert(leaders[self, view[self]] # -1, 
                         "Failure of assertion at line 127, column 9.")
               /\ IF leaders[self, view[self]] = self
                     THEN /\ pc' = [pc EXCEPT ![self] = "LeaderNode"]
                     ELSE /\ pc' = [pc EXCEPT ![self] = "FollowerNode"]
               /\ UNCHANGED << view, leaders, view_leaders, leader, msgs, _msg, 
                               next_leader >>

FollowerNode(self) == /\ pc[self] = "FollowerNode"
                      /\ \E m \in msgs: (m.type = "block" /\ m.view = view[self] + 1)
                      /\ _msg' = [_msg EXCEPT ![self] = CHOOSE m \in msgs: (m.type = "block" /\ m.view = view[self] + 1)]
                      /\ view' = [view EXCEPT ![self] = _msg'[self].view]
                      /\ next_leader' = [next_leader EXCEPT ![self] = _msg'[self].block]
                      /\ msgs' = (msgs \cup {[type |-> "vote", view |-> view'[self], block |-> _msg'[self].block , node |-> self]})
                      /\ leaders' = [leaders EXCEPT ![self, view'[self]] = next_leader'[self]]
                      /\ IF view'[self] < MaxView
                            THEN /\ pc' = [pc EXCEPT ![self] = "Begin"]
                            ELSE /\ pc' = [pc EXCEPT ![self] = "End"]
                      /\ UNCHANGED << view_leaders, leader >>

LeaderNode(self) == /\ pc[self] = "LeaderNode"
                    /\ view' = [view EXCEPT ![self] = view[self] + 1]
                    /\ view_leaders' = [view_leaders EXCEPT ![view'[self]] = view_leaders[view'[self]] + 1]
                    /\ \E b \in Block: BlockApproved(view'[self] - 1, b)
                    /\ next_leader' = [next_leader EXCEPT ![self] = leader[view'[self] + 1]]
                    /\ msgs' = (msgs \cup  {[type |-> "block", view |-> view'[self], block |-> next_leader'[self], node |-> self]})
                    /\ leaders' = [leaders EXCEPT ![self, view'[self]] = next_leader'[self]]
                    /\ IF view'[self] < MaxView
                          THEN /\ pc' = [pc EXCEPT ![self] = "Begin"]
                          ELSE /\ pc' = [pc EXCEPT ![self] = "End"]
                    /\ UNCHANGED << leader, _msg >>

End(self) == /\ pc[self] = "End"
             /\ TRUE
             /\ pc' = [pc EXCEPT ![self] = "Done"]
             /\ UNCHANGED << view, leaders, view_leaders, leader, msgs, _msg, 
                             next_leader >>

node(self) == Start(self) \/ Begin(self) \/ FollowerNode(self)
                 \/ LeaderNode(self) \/ End(self)

(* Allow infinite stuttering to prevent deadlock on termination. *)
Terminating == /\ \A self \in ProcSet: pc[self] = "Done"
               /\ UNCHANGED vars

Next == (\E self \in Node: node(self))
           \/ Terminating

Spec == /\ Init /\ [][Next]_vars
        /\ \A self \in Node : WF_vars(node(self))

Termination == <>(\A self \in ProcSet: pc[self] = "Done")

\* END TRANSLATION 



====
