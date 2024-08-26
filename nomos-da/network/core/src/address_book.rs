use libp2p::{Multiaddr, PeerId};
use std::collections::HashMap;
use std::sync::Arc;

/// Store for known peer addresses
/// It is a simple wrapper around a `HashMap` at the moment.
/// But it should be abstracted here to keep addresses in sync among different libp2p protocols
#[derive(Clone)]
pub struct AddressBook(Arc<HashMap<PeerId, Multiaddr>>);

impl AddressBook {
    pub fn empty() -> Self {
        Self(Arc::new(HashMap::new()))
    }

    pub fn get_address(&self, peer_id: &PeerId) -> Option<&Multiaddr> {
        self.0.get(peer_id)
    }
}

impl FromIterator<(PeerId, Multiaddr)> for AddressBook {
    fn from_iter<T: IntoIterator<Item = (PeerId, Multiaddr)>>(iter: T) -> Self {
        Self(Arc::new(iter.into_iter().collect()))
    }
}
