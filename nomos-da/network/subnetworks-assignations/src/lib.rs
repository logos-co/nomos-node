use std::collections::HashSet;
use std::hash::Hash;

pub trait MembershipHandler {
    // Subnetworks Id type
    type NetworkId: Eq + Hash;

    // Members Id type
    type Id;

    // Returns the set of NetworksIds an id is a member of
    fn membership(&self, id: &Self::Id) -> HashSet<Self::NetworkId>;

    // True if the id is a member of a network_id, False otherwise
    fn is_member_of(&self, id: &Self::Id, network_id: &Self::NetworkId) -> bool {
        self.membership(id).contains(network_id)
    }

    // Returns true if the member id is in the overall membership set
    fn is_allowed(&self, id: &Self::Id) -> bool;

    // Returns the set of members in a subnetwork by its NetworkId
    fn members_of(&self, network_id: &Self::NetworkId) -> HashSet<Self::Id>;
}
