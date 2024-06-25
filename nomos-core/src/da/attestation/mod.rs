use std::hash::Hash;

pub trait Attestation {
    type Hash: Hash + Eq + Clone;

    fn blob_hash(&self) -> Self::Hash;
    fn hash(&self) -> Self::Hash;
    fn signature(&self) -> &[u8];
}
