#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CommitteeId(pub(crate) [u8; 32]);

impl CommitteeId {
    pub const fn new(val: [u8; 32]) -> Self {
        Self(val)
    }
}

impl From<[u8; 32]> for CommitteeId {
    fn from(id: [u8; 32]) -> Self {
        Self(id)
    }
}

impl From<&[u8; 32]> for CommitteeId {
    fn from(id: &[u8; 32]) -> Self {
        Self(*id)
    }
}

impl From<CommitteeId> for [u8; 32] {
    fn from(id: CommitteeId) -> Self {
        id.0
    }
}

impl core::fmt::Display for CommitteeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x")?;
        for v in self.0 {
            write!(f, "{:02x}", v)?;
        }
        Ok(())
    }
}
