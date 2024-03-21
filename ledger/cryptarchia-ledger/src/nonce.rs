use crate::utils::serialize_bytes_newtype;

#[derive(Clone, Debug, Eq, PartialEq, Copy)]
pub struct Nonce([u8; 32]);
impl From<[u8; 32]> for Nonce {
    fn from(nonce: [u8; 32]) -> Self {
        Self(nonce)
    }
}

impl From<Nonce> for [u8; 32] {
    fn from(nonce: Nonce) -> [u8; 32] {
        nonce.0
    }
}

impl AsRef<[u8]> for Nonce {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
serialize_bytes_newtype!(Nonce);
