// std
use std::marker::PhantomData;
// crates

use crate::da::attestation::Attestation;
use crate::da::auth::Verifier;
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
    ) -> impl Iterator<Item = Self::Certificate> + 'i {
        utils::select::select_from_till_fill_size::<SIZE, Self::Certificate>(
            |blob| blob.as_bytes().len(),
            certificates,
        )
    }
}

pub trait KeyStore {
    type Key: Clone;
    type Item: Clone;

    fn get_key(&self, node_id: &Self::Key) -> Option<&Self::Item>;
}

#[derive(Default, Clone, Copy)]
pub struct FillVerifiedSize<const SIZE: usize, B, KS: KeyStore> {
    fill_size: FillSize<SIZE, B>,
    key_store: KS,
}

impl<const SIZE: usize, B, KS> FillVerifiedSize<SIZE, B, KS>
where
    KS: KeyStore,
    KS::Key: Clone,
{
    pub fn new(key_store: KS) -> Self {
        Self {
            fill_size: FillSize::new(),
            key_store,
        }
    }
}

impl<const SIZE: usize, C, KS> BlobCertificateSelect for FillVerifiedSize<SIZE, C, KS>
where
    C: Certificate + Clone,
    <<C as Certificate>::Attestation as Attestation>::Voter: Clone,
    KS: KeyStore<Key = <C::Attestation as Attestation>::Voter, Item = dyn Verifier>
        + Default
        + Clone
        + 'static,
{
    type Certificate = C;
    type Settings = ();

    fn new(_settings: Self::Settings) -> Self {
        FillVerifiedSize::new(KS::default())
    }

    fn select_blob_from<'i, I: Iterator<Item = Self::Certificate> + 'i>(
        &self,
        certificates: I,
    ) -> impl Iterator<Item = Self::Certificate> + 'i {
        let key_store_clone = self.key_store.clone();
        certificates.filter_map(move |certificate| {
            if certificate.attestations().iter().any(|attestation| {
                key_store_clone
                    .get_key(&attestation.voter())
                    .and_then(|voter_key| {
                        if voter_key.verify(&attestation.as_bytes()) {
                            Some(())
                        } else {
                            None
                        }
                    })
                    .is_some()
            }) {
                Some(certificate)
            } else {
                None
            }
        })
    }
}
