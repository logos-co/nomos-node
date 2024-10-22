// std
use std::marker::PhantomData;
// crates

// internal
use crate::da::blob::{info::DispersedBlobInfo, BlobSelect};
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

impl<const SIZE: usize, B: DispersedBlobInfo> BlobSelect for FillSize<SIZE, B> {
    type BlobId = B;

    type Settings = ();

    fn new(_settings: Self::Settings) -> Self {
        FillSize::new()
    }

    fn select_blob_from<'i, I: Iterator<Item = Self::BlobId> + 'i>(
        &self,
        certificates: I,
    ) -> impl Iterator<Item = Self::BlobId> + 'i {
        utils::select::select_from_till_fill_size::<SIZE, Self::BlobId>(|c| c.size(), certificates)
    }
}
