use bytes::{Bytes, BytesMut};

pub const DA_VID_KEY_PREFIX: &str = "da/vid/";
pub const DA_VERIFIED_KEY_PREFIX: &str = "da/verified/";
pub const DA_BLOB_PATH: &str = "bl";
pub const DA_SHARED_COMMITMENTS_PATH: &str = "sc";
pub const DA_ATTESTATION_PATH: &str = "at";

pub fn key_bytes(prefix: &str, id: impl AsRef<[u8]>) -> Bytes {
    let mut buffer = BytesMut::new();

    buffer.extend_from_slice(prefix.as_bytes());
    buffer.extend_from_slice(id.as_ref());

    buffer.freeze()
}
