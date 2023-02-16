---- MODULE MCcarnot_flat ----
EXTENDS carnot_flat



ConstNode == 1..3
ConstQuorum == { q \in SUBSET ConstNode : Cardinality(q) * 3 >= Cardinality(ConstNode) * 2 }
ConstMaxView == 3

====
