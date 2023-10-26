#[derive(Clone, Copy, Default, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct NodeId(pub(crate) [u8; 32]);

#[cfg(feature = "serde")]
impl serde::Serialize for NodeId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        nomos_utils::serde::serialize_bytes_array(self.0, serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::de::Deserialize<'de> for NodeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        nomos_utils::serde::deserialize_bytes_array(deserializer).map(Self)
    }
}

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

impl<'a> From<&'a NodeId> for &'a [u8; 32] {
    fn from(id: &'a NodeId) -> Self {
        &id.0
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
