pub mod mock;

pub trait Signer {
    fn sign(&self, message: &[u8]) -> Vec<u8>;
}

pub trait DaAuth {
    type Settings: Clone;

    fn new(settings: Self::Settings) -> Self;
}
