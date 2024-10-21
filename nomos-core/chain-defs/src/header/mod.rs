use serde::{Deserialize, Serialize};

use crate::utils::{display_hex_bytes_newtype, serde_bytes_newtype};
pub mod cryptarchia;

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash, PartialOrd, Ord)]
pub struct HeaderId([u8; 32]);

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash)]
pub struct ContentId([u8; 32]);

// This lint is a false positive?
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Header {
    Cryptarchia(cryptarchia::Header),
}

impl Header {
    pub fn cryptarchia(&self) -> &cryptarchia::Header {
        match self {
            Self::Cryptarchia(header) => header,
        }
    }

    pub fn id(&self) -> HeaderId {
        match self {
            Self::Cryptarchia(header) => header.id(),
        }
    }

    pub fn parent(&self) -> HeaderId {
        match self {
            Self::Cryptarchia(header) => header.parent(),
        }
    }
}

impl From<[u8; 32]> for HeaderId {
    fn from(id: [u8; 32]) -> Self {
        Self(id)
    }
}

impl From<HeaderId> for [u8; 32] {
    fn from(id: HeaderId) -> Self {
        id.0
    }
}

impl From<[u8; 32]> for ContentId {
    fn from(id: [u8; 32]) -> Self {
        Self(id)
    }
}

impl From<ContentId> for [u8; 32] {
    fn from(id: ContentId) -> Self {
        id.0
    }
}

display_hex_bytes_newtype!(HeaderId);
display_hex_bytes_newtype!(ContentId);

serde_bytes_newtype!(HeaderId, 32);
serde_bytes_newtype!(ContentId, 32);

#[test]
fn test_serde() {
    assert_eq!(
        crate::wire::deserialize::<HeaderId>(&crate::wire::serialize(&HeaderId([0; 32])).unwrap())
            .unwrap(),
        HeaderId([0; 32])
    );
}
