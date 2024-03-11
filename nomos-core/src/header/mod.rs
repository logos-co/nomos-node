use serde::{Deserialize, Serialize};

use crate::utils::display_hex_bytes_newtype;

pub mod carnot;
pub mod cryptarchia;

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct HeaderId([u8; 32]);

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash, Serialize, Deserialize)]
pub struct ContentId([u8; 32]);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Header {
    Cryptarchia(cryptarchia::Header),
    Carnot(carnot::Header),
}

impl Header {
    pub fn cryptarchia(&self) -> &cryptarchia::Header {
        match self {
            Self::Cryptarchia(header) => header,
            Self::Carnot(_) => panic!("Header is not a Cryptarchia header"),
        }
    }

    pub fn carnot(&self) -> &carnot::Header {
        match self {
            Self::Carnot(header) => header,
            Self::Cryptarchia(_) => panic!("Header is not a Carnot header"),
        }
    }

    pub fn id(&self) -> HeaderId {
        match self {
            Self::Cryptarchia(header) => header.id(),
            Self::Carnot(header) => header.id(),
        }
    }

    pub fn parent(&self) -> HeaderId {
        match self {
            Self::Cryptarchia(header) => header.parent(),
            Self::Carnot(header) => header.parent(),
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
