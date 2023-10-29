/// The block id
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Ord, PartialOrd)]

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BlockId(pub(crate) [u8; 32]);

#[cfg(feature = "serde")]
impl serde::Serialize for BlockId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        nomos_utils::serde::serialize_bytes_array(self.0, serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::de::Deserialize<'de> for BlockId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        nomos_utils::serde::deserialize_bytes_array(deserializer).map(Self)
    }
}

impl BlockId {
    pub const fn new(val: [u8; 32]) -> Self {
        Self(val)
    }

    pub const fn zeros() -> Self {
        Self([0; 32])
    }

    /// Returns a random block id, only avaliable with feature `simulation` or test
    #[cfg(any(test, feature = "simulation"))]
    pub fn random<R: rand::Rng>(rng: &mut R) -> Self {
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        Self(bytes)
    }
}

impl From<[u8; 32]> for BlockId {
    fn from(id: [u8; 32]) -> Self {
        Self(id)
    }
}

impl From<&[u8; 32]> for BlockId {
    fn from(id: &[u8; 32]) -> Self {
        Self(*id)
    }
}

impl From<BlockId> for [u8; 32] {
    fn from(id: BlockId) -> Self {
        id.0
    }
}

impl<'a> From<&'a BlockId> for &'a [u8; 32] {
    fn from(id: &'a BlockId) -> Self {
        &id.0
    }
}

impl core::fmt::Display for BlockId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x")?;
        for v in self.0 {
            write!(f, "{:02x}", v)?;
        }
        Ok(())
    }
}
