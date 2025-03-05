use bytes::{Bytes, BytesMut};

// "DA/VID/" PREFIX
pub const DA_VID_KEY_PREFIX: &str = "da/vid/";

// "DA/VERIFIED/" PREFIX
pub const DA_SHARED_COMMITMENTS_PREFIX: &str = concat!("da/verified/", "sc");
pub const DA_BLOB_PREFIX: &str = concat!("da/verified/", "bl");

pub fn key_bytes<Id: AsRef<[u8]>>(prefix: &str, id: Id) -> Bytes {
    let mut buffer = BytesMut::new();

    buffer.extend_from_slice(prefix.as_bytes());
    buffer.extend_from_slice(id.as_ref());

    buffer.freeze()
}

// Combines a 32-byte blob ID (`[u8; 32]`) with a 2-byte column index
// (`u16` represented as `[u8; 2]`).
#[must_use]
pub fn create_blob_idx(blob_id: &[u8], column_idx: &[u8]) -> [u8; 34] {
    let mut blob_idx = [0u8; 34];
    blob_idx[..blob_id.len()].copy_from_slice(blob_id);
    blob_idx[blob_id.len()..].copy_from_slice(column_idx);

    blob_idx
}
