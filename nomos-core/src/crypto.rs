use blake2::digest::typenum::U32;

pub type PublicKey = [u8; 32];
pub type PrivateKey = [u8; 32];
pub type Signature = [u8; 32];

pub type Blake2b = blake2::Blake2b<U32>;
