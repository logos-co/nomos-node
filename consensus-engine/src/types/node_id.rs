#[derive(Clone, Copy, Default, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct NodeId(
    #[cfg_attr(feature = "serde", serde(with = "crate::util::serde_array32"))] pub(crate) [u8; 32],
);

impl NodeId {
    pub const fn new(val: [u8; 32]) -> Self {
        Self(val)
    }

    /// Returns a random node id
    #[cfg(any(test, feature = "simulation"))]
    pub fn random<R: rand::Rng>(rng: &mut R) -> Self {
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

impl core::fmt::Display for NodeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x")?;
        for v in self.0 {
            write!(f, "{:02x}", v)?;
        }
        Ok(())
    }
}
