use bytes::{Bytes, BytesMut};

pub const DA_VID_KEY_PREFIX: &str = "da/vid/";
pub const DA_ATTESTED_KEY_PREFIX: &str = "da/attested/";

pub fn key_bytes(prefix: &str, id: impl AsRef<[u8]>) -> Bytes {
    let mut buffer = BytesMut::new();

    buffer.extend_from_slice(prefix.as_bytes());
    buffer.extend_from_slice(id.as_ref());

    buffer.freeze()
}
