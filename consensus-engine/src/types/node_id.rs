#[derive(Clone, Copy, Default, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct NodeId(pub(crate) [u8; 32]);

impl NodeId {
    pub const fn new(val: [u8; 32]) -> Self {
        Self(val)
    }

    /// Returns a random node id
    #[cfg(any(test, feature = "simulation"))]
    pub fn random() -> Self {
        use rand::RngCore;
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        Self(bytes)
    }
}

impl From<[u8; 32]> for NodeId {
    fn from(id: [u8; 32]) -> Self {
        Self(id)
    }
}

impl From<&[u8; 32]> for NodeId {
    fn from(id: &[u8; 32]) -> Self {
        Self(*id)
    }
}

impl From<NodeId> for [u8; 32] {
    fn from(id: NodeId) -> Self {
        id.0
    }
}

impl From<super::CommitteeId> for NodeId {
    /// A node id should be able to built from committee id
    fn from(id: super::CommitteeId) -> Self {
        Self(id.into())
    }
}

impl From<&super::CommitteeId> for NodeId {
    /// A node id should be able to built from committee id
    fn from(id: &super::CommitteeId) -> Self {
        Self((*id).into())
    }
}

impl core::fmt::Display for NodeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x")?;
        for v in self.0 {
            write!(f, "{:02x}", v)?;
        }
        Ok(())
    }
}
