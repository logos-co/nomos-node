pub trait Signer {
    fn sign(&self, message: &[u8]) -> Vec<u8>;
}

pub trait Verifier {
    fn verify(&self, message: &[u8]) -> bool;
}
