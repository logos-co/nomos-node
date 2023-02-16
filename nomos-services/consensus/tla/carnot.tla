------------------------------ MODULE carnot ------------------------------- 
(***************************************************************************)
(* This is a high-level description of Carnot, trying to abstract          *)
(* details irrelevant to the end consensus like committees, overlay and    *)
(* networking.                                                             *)
(* TODO: this only models hones nodes.                                     *)
(***************************************************************************)
EXTENDS Integers, FiniteSets

(***************************************************************************)
(* This follows the specifications of Voting in Lamport's presentation at  *)
(* SPTDC. We now declare the set Block of values that can be chosen as the *)
(*  next block, the set Node of nodes, and another set Quorum that is      *)
(* a set of the sets of nodes which form a quorum.                         *)
(***************************************************************************)
CONSTANTS Node, Quorum, MaxView, Block

(***************************************************************************)
(* The following assumption asserts that Quorum is a set of subsets of the *)
(* set Node, and that it contains at least 2/3 of acceptors                *)
(***************************************************************************)
ASSUME /\ \A Q \in Quorum : Q \subseteq Node
       /\ \A Q : Cardinality(Q) * 3 >= Cardinality(Node) * 2

View == Nat
-----------------------------------------------------------------------------
(***************************************************************************)
(* The algorithm works by having nodes cast votes in numbered views.       *)
(* Each node can cast one or more votes, where each vote cast by a node    *)
(* has the form <<v, b>> indicating that the node  has voted for block b   *)
(* in view  b. A block is chosen if a quorum of acceptors have voted for   *) 
(* it in the same view  .                                                  *)
(*                                                                         *)
(* We now declare the algorithm's variables 'votes', 'proposals' and       *)
(* 'leaders'. For each node n, the value of votes[n] is the set of         *)
(* votes cast by n and proposals[n] the set of blocks proposed by n.       *)
(* For each view v leader[v] is the output of the magic oracle determining *)
(* the leader for view v.                                                  *)
(***************************************************************************)
VARIABLES votes, proposals, leader

(***************************************************************************)
(* TypeOK asserts the "types" of the variables, represented as functions   *)
(***************************************************************************)
CarnotTypeOK ==
   /\ votes \in [Node -> SUBSET (View \X Block)]
   /\ proposals \in [Node -> SUBSET (View \X Block)]
\*    /\ leader \in [View -> Node]

(***************************************************************************)
(* Next comes a sequence of definitions of concepts used to explain the    *)
(* algorithm.                                                              *)
(***************************************************************************)

Approved(n, v, b) == <<v, b>> \in votes[n]
    (************************************************************************)
    (* True iff (if and only if) node n has approved block b in view v      *)
    (************************************************************************)

ChosenAt(v, b) == 
   \E Q \in Quorum : \A n \in Q : Approved(n, v, b)
   (************************************************************************)
   (* True iff a quorum of acceptors have all approved block b in view v   *)
   (************************************************************************)

LeaderProposed(n, v, b) == \/ v = 0 \* special case for genesis block
                           \/ \E prop \in proposals[leader[v]]: 
                                /\ prop[1] = v
                                /\ prop[2] = b 
    (************************************************************************)
    (* True iff (if and only if) the leader for view v has proposed block b *)
    (* in view v. View 0 is treated specially as the genesis block is       *)
    (* agreed beforehand.                                                   *)
    (*                                                                      *)
    (* It accepts the node n asking for the information as a malicious      *)
    (* leader could send different proposals to different nodes, even       *)
    (* though this is not implemented yet.                                  *)
    (************************************************************************)

OnlyVoteForProposedBlocks == 
    \A n \in Node, v \in 0..MaxView, b \in Block:
        Approved(n, v, b) => \E l \in Node: LeaderProposed(l, v, b)
    (************************************************************************)
    (* Honest nodes should only approve blocks that has been proposed by    *)
    (* the leader.                                                          *)
    (************************************************************************)

ParentIsValid == 
    \A n \in Node, v \in 1..MaxView, b \in Block:
        LeaderProposed(n, v, b) => \E parent \in Block: ChosenAt(v-1, parent)
    (************************************************************************)
    (* Honest leaders should only propose blocks whose parent has been      *)
    (* approved by a quorum                                                 *)
    (************************************************************************)

chosen == {
            <<approved[1], approved[2]>> : 
                approved \in { 
                    proposal \in ((0..MaxView) \X Block):
                        ChosenAt(proposal[1], proposal[2])
                }
          }
   (************************************************************************)
   (* Defines `chosen' to be the set of all tuples <<v, b>> for which      *)
   (* ChosenAt(v, b) is true. It represents the achieved consensus on      *)
   (* proposals                                                            *)
   (************************************************************************)

(***************************************************************************)
(* Next comes a sequence of properties that 'chosen' should satisfy        *)
(***************************************************************************)

AppendOnly ==
    \E v \in 0..MaxView, b \in Block: chosen' = chosen \cup {<<v, b>>}
    (***********************************************************************)
    (* The set of blocks on which consensus was reached is append only,    *)
    (* i.e. we can never roll back any decision.                           *)
    (* In practice, if at some point ChosenAt(v, b) is true it will always *)
    (* be true for the rest of the execution                               *)
    (***********************************************************************)
   
OneBlockPerView ==  
    \A c1 \in chosen : ~\E c2 \in chosen \ {c1} : c1[1] = c2[1]
    (************************************************************************)
    (* Consensus can only select at most one block for each view            *)
    (************************************************************************)

SharedEnd == Cardinality(chosen) = MaxView
    (************************************************************************)
    (* Liveness condition: Carnot should select a block for each view       *)
    (************************************************************************)

(***************************************************************************)
(* Next come the two main actions in the Carnot algorithm.                 *)
(***************************************************************************)

(***************************************************************************)
(* A node n form a proposal for block b in view number v.                  *)
(* The enabling condition contains the following conjuncts:                *)
(*                                                                         *)
(*    1) Node n is a leader for view b                                     *)
(*    2) Consensus was achieved for the parent block (simplified)          *)
(*    3) It did not already propose a block for view v                     *)
(*                                                                         *)
(* A TLA+ primer:                                                          *)
(*    - A tuple t is a function (array) whose first element is t[1], whose *)
(*      second element if t[2], and so on. Thus, a proposal p is the pair  *)
(*      <<vt[1], vt[2]>>                                                   *)
(*    - x' is the value of x at the end of the step                        *)
(*    - v'=[v EXCEPT ![n] = p]' means v' is the same as v except v[n] = p  *)
(***************************************************************************)
ProposeBlock(n, v, b) ==
    /\ leader[v] = n                     \* 1)
    /\ \E b1 \in Block: ChosenAt(v-1, b1)\* 2) for happy path only
    /\ \A p \in proposals[n] : p[1] /= v \* 3)
    /\ proposals' = [proposals EXCEPT ![n] = proposals[n] \cup {<<v, b>>}]


(***************************************************************************)
(* A node n approves a proposal for block b in view number v.              *)
(* The enabling condition contains the following conjuncts:                *)
(*                                                                         *)
(*    1) A leader proposed block b for view v.                             *)
(*    2) It did not already vote for a block in view v.                    *)
(*    3) TODO: decide how to model children approval                       *)
(***************************************************************************)
ApproveBlock(n, v, b) ==
    /\ LeaderProposed(n, v, b)           \* 1)
    /\ \A vt \in votes[n] : vt[1] /= v   \* 2)
    /\ TRUE                              \* 3) TODO
    /\ votes' = [votes EXCEPT ![n] = votes[n] \cup {<<v, b>>}]

-----------------------------------------------------------------------------
(***************************************************************************)
(* Finally, we get to the definition of the algorithm.  The initial        *)
(* predicate is obvious.                                                   *)
(***************************************************************************)
CarnotInit == /\ votes  = [n \in Node |-> {}]
              /\ proposals = [n \in Node |-> {}]
              
(***************************************************************************)
(* The algorithm advances when either a leader proposes a block or a node  *)
(* approves a proposal.                                                    *)
(***************************************************************************)
CarnotNext  == \E n \in Node, v \in 0..MaxView, b \in Block : 
                \/ ProposeBlock(n, v, b)
                \/ ApproveBlock(n, v, b)

(***************************************************************************)
(* This is the specification of Carnot. It contains the starting predicates*)
(* and allowed actions.                                                    *)
(* In TLA+, [P]_<v> means check that property P is true if the value of v  *)
(* has changes,formally: P \/ UNCHANGED v.                                 *)
(* In TLA+, '[]' means 'always'. Without it CarnotNext would only be       *)
(* for the initial state.                                                  *)
(***************************************************************************)
CarnotSpec == CarnotInit /\ [][CarnotNext]_<<votes, proposals>>

(***************************************************************************)
(* Simple invariant to check programming erors.                            *)
(***************************************************************************)
CarnotInv == CarnotTypeOK

(***************************************************************************)
(* Safety properties of Carnot:                                            *)
(*                                                                         *)
(*    1) There can't be more than one block chosen by consensus per view   *)
(*    2) A node should only approve blocks that has been proposed by a     *)
(*       leader                                                            *)
(*    2) A leader should only propose blocks whose parent has been approved*)
(*       by a quorum.                                                      *)
(*    3) The set of blocks on which consensus is reached can only grow     *)
(***************************************************************************)
CarnotSafety == /\ []OneBlockPerView 
                /\ []OnlyVoteForProposedBlocks
                /\ []ParentIsValid
                /\ [][AppendOnly]_chosen 
                
(***************************************************************************)
(* Safety properties of Carnot:                                            *)
(*    1) Eventually a consensus is reached on each view.                   *)
(* In TLA+, '<>' means 'eventually'                                        *)
(***************************************************************************)
CarnotLiveness == <>SharedEnd 
=============================================================================