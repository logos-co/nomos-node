use crate::{BYTES_PER_COMMITMENT, BYTES_PER_PROOF};
use kzg::types::fr::FsFr;
use kzg::types::g1::FsG1;
use kzg::types::kzg_settings::FsKZGSettings;
use kzg_traits::{Fr, G1};
use std::error::Error;

pub struct KzgSettings {
    pub settings: FsKZGSettings,
    pub bytes_per_field_element: usize,
}

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
    pub fn from_bytes(data: &[u8], settings: &KzgSettings) -> Result<Self, Box<dyn Error>> {
        let mut inner = Vec::with_capacity(data.len() / settings.bytes_per_field_element);
        for chunk in data.chunks(settings.bytes_per_field_element) {
            inner.push(FsFr::from_bytes(chunk)?);
        }
        Ok(Self { inner })
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn inner(&self) -> Vec<FsFr> {
        self.inner.clone()
    }
}
