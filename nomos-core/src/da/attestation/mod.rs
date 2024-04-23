use std::hash::Hash;

pub trait Attestation {
    type Attester;
    type Hash: Hash + Eq + Clone;

    fn attester(&self) -> Self::Attester;
    fn blob_hash(&self) -> Self::Hash;
    fn hash(&self) -> Self::Hash;
    fn signature(&self) -> &[u8];
}
