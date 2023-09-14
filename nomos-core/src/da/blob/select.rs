// std
use std::marker::PhantomData;
// crates

// internal
use crate::da::blob::{Blob, BlobSelect};
use crate::utils;

#[derive(Default)]
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

impl<const SIZE: usize, B: Blob> BlobSelect for FillSize<SIZE, B> {
    type Blob = B;

    fn select_blob_from<'i, I: Iterator<Item = Self::Blob> + 'i>(
        &self,
        blobs: I,
    ) -> Box<dyn Iterator<Item = Self::Blob> + 'i> {
        utils::select::select_from_till_fill_size::<SIZE, Self::Blob>(
            |blob| blob.as_bytes().len(),
            blobs,
        )
    }
}
