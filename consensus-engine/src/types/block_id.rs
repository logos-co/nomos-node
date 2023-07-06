#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct BlockId(pub(crate) [u8; 32]);

impl BlockId {
    #[inline]
    pub const fn new(val: [u8; 32]) -> Self {
        Self(val)
    }

    #[inline]
    pub const fn genesis() -> Self {
        Self([0; 32])
    }

    /// Returns a random block id
    #[inline]
    pub fn random() -> Self {
        use rand::RngCore;
        let mut rng = rand::thread_rng();
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

impl core::fmt::Display for BlockId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x")?;
        for v in self.0 {
            write!(f, "{:02x}", v)?;
        }
        Ok(())
    }
}
