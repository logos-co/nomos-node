#[derive(Clone, Copy, Default, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct NodeId(pub(crate) [u8; 32]);

impl NodeId {
    #[inline]
    pub const fn new(val: [u8; 32]) -> Self {
        Self(val)
    }

    /// Returns a random node id
    #[cfg(any(test, feature = "simulation"))]
    #[inline]
    pub fn random() -> Self {
        use rand::RngCore;
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    /// Returns the index of the node, the index is the first [0..size of usize] bytes of the node id
    #[cfg(any(test, feature = "simulation"))]
    #[inline]
    pub fn index(&self) -> usize {
        const SIZE: usize = core::mem::size_of::<usize>();
        let mut bytes = [0u8; SIZE];
        bytes.copy_from_slice(&self.0[..SIZE]);
        usize::from_be_bytes(bytes)
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

// A node id should be able to built from committee id
impl From<super::CommitteeId> for NodeId {
    fn from(id: super::CommitteeId) -> Self {
        Self(id.into())
    }
}

// A node id should be able to built from committee id
impl From<&super::CommitteeId> for NodeId {
    fn from(id: &super::CommitteeId) -> Self {
        Self((*id).into())
    }
}

// A convienient way to build a node id from an index
//
// The format is:
//
// [0..size of usize]: node index in big endian
// [size of usize..32]: zeros
#[cfg(any(test, feature = "simulation"))]
impl From<usize> for NodeId {
    fn from(id: usize) -> Self {
        let mut bytes = [0u8; 32];
        bytes[..core::mem::size_of::<usize>()].copy_from_slice(&id.to_be_bytes());
        Self(bytes)
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
