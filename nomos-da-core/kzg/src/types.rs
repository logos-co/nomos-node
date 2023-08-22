use super::BYTES_PER_FIELD_ELEMENT;
use crate::{BYTES_PER_COMMITMENT, BYTES_PER_PROOF};
use kzg::types::fr::FsFr;
use kzg::types::g1::FsG1;
use kzg_traits::{Fr, G1};
use std::error::Error;

pub struct Commitment(pub(crate) FsG1);

pub struct Proof(pub(crate) FsG1);

pub struct Blob {
    pub(crate) inner: Vec<FsFr>,
}

impl Commitment {
    pub fn as_bytes_owned(&self) -> [u8; BYTES_PER_COMMITMENT] {
        self.0.to_bytes()
    }
}

impl Proof {
    pub fn as_bytes_owned(&self) -> [u8; BYTES_PER_PROOF] {
        self.0.to_bytes()
    }
}

impl Blob {
    pub fn from_bytes(data: &[u8]) -> Result<Self, Box<dyn Error>> {
        let mut inner = Vec::with_capacity(data.len() / BYTES_PER_FIELD_ELEMENT);
        for chunk in data.chunks(BYTES_PER_FIELD_ELEMENT) {
            inner.push(FsFr::from_bytes(chunk)?);
        }
        Ok(Self { inner })
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn inner(&self) -> Vec<FsFr> {
        self.inner.clone()
    }
}
