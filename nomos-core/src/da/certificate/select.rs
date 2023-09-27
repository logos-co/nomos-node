// std
use std::marker::PhantomData;
// crates

// internal
use crate::da::certificate::{BlobCertificateSelect, Certificate};
use crate::utils;

#[derive(Default, Clone, Copy)]
pub struct FillSize<const SIZE: usize, B> {
    _blob: PhantomData<B>,
}

impl<const SIZE: usize, B> FillSize<SIZE, B> {
    pub fn new() -> Self {
        Self {
            _blob: Default::default(),
        }
    }
}

impl<const SIZE: usize, C: Certificate> BlobCertificateSelect for FillSize<SIZE, C> {
    type Certificate = C;
    type Settings = ();

    fn new(_settings: Self::Settings) -> Self {
        FillSize::new()
    }

    fn select_blob_from<'i, I: Iterator<Item = Self::Certificate> + 'i>(
        &self,
        certificates: I,
    ) -> Box<dyn Iterator<Item = Self::Certificate> + 'i> {
        utils::select::select_from_till_fill_size::<SIZE, Self::Certificate>(
            |blob| blob.as_bytes().len(),
            certificates,
        )
    }
}
