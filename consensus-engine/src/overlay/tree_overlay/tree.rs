use crate::{Committee, NodeId};
use blake2::{digest::typenum::U32, Blake2b, Digest};
use std::collections::{HashMap, HashSet};

fn blake2b_hash(committee: &Committee) -> [u8; 32] {
    let mut hasher = Blake2b::<U32>::new();
    for member in committee {
        hasher.update(member);
    }
    hasher.finalize().into()
}

#[derive(Debug, Clone)]
pub(super) struct Tree {
    pub(super) inner_committees: Vec<NodeId>,
    pub(super) membership_committees: HashMap<usize, Committee>,
    pub(super) committee_id_to_index: HashMap<NodeId, usize>,
    pub(super) committees_by_member: HashMap<NodeId, usize>,
}

impl Tree {
    pub fn new(nodes: &[NodeId], number_of_committees: usize) -> Self {
        assert!(number_of_committees > 0);

        let (inner_committees, membership_committees) =
            Self::build_committee_from_nodes_with_size(nodes, number_of_committees);

        let committee_id_to_index = inner_committees
            .iter()
            .copied()
            .enumerate()
            .map(|(idx, c)| (c, idx))
            .collect();

        let committees_by_member = membership_committees
            .iter()
            .flat_map(|(committee, members)| members.iter().map(|member| (*member, *committee)))
            .collect();

        Self {
            inner_committees,
            membership_committees,
            committee_id_to_index,
            committees_by_member,
        }
    }

    pub(super) fn build_committee_from_nodes_with_size(
        nodes: &[NodeId],
        number_of_committees: usize,
    ) -> (Vec<NodeId>, HashMap<usize, Committee>) {
        let committee_size = nodes.len() / number_of_committees;
        let remainder = nodes.len() % number_of_committees;

        let mut committees: Vec<HashSet<NodeId>> = (0..number_of_committees)
            .map(|n| {
                nodes[n * committee_size..(n + 1) * committee_size]
                    .iter()
                    .cloned()
                    .collect()
            })
            .collect();

        // Refill committees with extra nodes
        if remainder != 0 {
            for i in 0..remainder {
                let node = nodes[nodes.len() - remainder + i];
                let committee_index = i % number_of_committees;
                committees[committee_index].insert(node);
            }
        }

        let hashes = committees.iter().map(blake2b_hash).collect::<Vec<_>>();
        (hashes, committees.into_iter().enumerate().collect())
    }

    pub(super) fn parent_committee(&self, committee_id: &NodeId) -> Option<&NodeId> {
        if committee_id == &self.inner_committees[0] {
            None
        } else {
            self.committee_id_to_index
                .get(committee_id)
                .map(|&idx| &self.inner_committees[(idx / 2).saturating_sub(1)])
        }
    }

    pub(super) fn parent_committee_from_member_id(&self, id: &NodeId) -> Committee {
        let Some(committee_id) = self.committee_id_by_member_id(id) else { return HashSet::new(); };
        let Some(parent_id) = self.parent_committee(committee_id) else { return HashSet::new(); };
        self.committee_by_committee_idx(self.committee_id_to_index[parent_id])
            .cloned()
            .unwrap_or_default()
    }

    pub(super) fn child_committees(
        &self,
        committee_id: &NodeId,
    ) -> (Option<&NodeId>, Option<&NodeId>) {
        let base = self
            .committee_id_to_index
            .get(committee_id)
            .map(|&idx| idx * 2)
            .unwrap_or(0);
        let first_child = base + 1;
        let second_child = base + 2;
        (
            self.inner_committees.get(first_child),
            self.inner_committees.get(second_child),
        )
    }

    pub(super) fn leaf_committees(&self) -> HashMap<&NodeId, &Committee> {
        let total_leafs = (self.inner_committees.len() + 1) / 2;
        let mut leaf_committees = HashMap::new();
        for i in (self.inner_committees.len() - total_leafs)..self.inner_committees.len() {
            leaf_committees.insert(&self.inner_committees[i], &self.membership_committees[&i]);
        }
        leaf_committees
    }

    pub(super) fn root_committee(&self) -> &Committee {
        &self.membership_committees[&0]
    }

    pub(super) fn committee_by_committee_idx(&self, committee_idx: usize) -> Option<&Committee> {
        self.membership_committees.get(&committee_idx)
    }

    pub(super) fn committee_idx_by_member_id(&self, member_id: &NodeId) -> Option<usize> {
        self.committees_by_member.get(member_id).copied()
    }

    pub(super) fn committee_id_by_member_id(&self, member_id: &NodeId) -> Option<&NodeId> {
        self.committees_by_member
            .get(member_id)
            .map(|&idx| &self.inner_committees[idx])
    }

    pub(super) fn committee_by_member_id(&self, member_id: &NodeId) -> Option<&Committee> {
        self.committee_idx_by_member_id(member_id)
            .and_then(|idx| self.committee_by_committee_idx(idx))
    }

    pub(super) fn committee_by_committee_id(&self, committee_id: &NodeId) -> Option<&Committee> {
        self.committee_id_to_index
            .get(committee_id)
            .and_then(|&idx| self.committee_by_committee_idx(idx))
    }
}
