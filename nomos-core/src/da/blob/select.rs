// std
use std::marker::PhantomData;
// crates

// internal
use crate::da::blob::{Blob, BlobSelect};
use crate::utils;

pub struct FillSize<const SIZE: usize, B> {
    _tx: PhantomData<B>,
}

impl<const SIZE: usize, B: Blob> BlobSelect for FillSize<SIZE, B> {
    type Blob = B;

    fn select_blob_from<'i, I: Iterator<Item = Self::Blob> + 'i>(
        &self,
        blobs: I,
    ) -> Box<dyn Iterator<Item = Self::Blob> + 'i> {
        utils::select::select_from_till_fill_size::<SIZE, Self::Blob>(
            |tx| tx.as_bytes().len(),
            blobs,
        )
    }
}
