use blake2::digest::typenum::U32;

pub(crate) type Blake2b = blake2::Blake2b<U32>;
