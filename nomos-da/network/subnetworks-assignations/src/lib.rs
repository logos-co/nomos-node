use std::collections::HashSet;
use std::hash::Hash;

pub trait MembershipHandler {
    type NetworkId: Eq + Hash;
    type Id;
    fn membership(&self, self_id: &Self::Id) -> HashSet<Self::NetworkId>;
    fn is_member_of(&self, id: &Self::Id, network_id: &Self::NetworkId) -> bool {
        self.membership(id).contains(network_id)
    }
}
