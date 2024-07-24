use std::collections::HashSet;

pub trait Designator {
    type NetworkId;
    type Id;
    fn membership(&self, self_id: Self::Id) -> HashSet<Self::NetworkId>;
    fn is_member_of(&self, id: Self::Id, network_id: Self::NetworkId) -> bool {
        self.membership(&id).contains(&network_id)
    }
}
