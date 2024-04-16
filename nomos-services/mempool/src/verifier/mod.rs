pub mod certificate;
pub mod tx;

pub trait Verifier<T> {
    type Settings: Clone;

    fn new(settings: Self::Settings) -> Self;
    fn verify(&self, item: &T) -> bool;
}
