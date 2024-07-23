use std::io;

use bytes::Bytes;
use futures::AsyncReadExt;
use prost::Message;

pub mod dispersal;

const MAX_MSG_LEN_BYTES: usize = 2;

pub fn pack_message(message: &impl Message) -> Result<Vec<u8>, io::Error> {
    let data_len = message.encoded_len();

    if data_len > (1 << (MAX_MSG_LEN_BYTES * 8)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Message too large",
        ));
    }

    let mut buf = Vec::with_capacity(MAX_MSG_LEN_BYTES + data_len);
    buf.extend_from_slice(&(data_len as u16).to_be_bytes());
    message.encode(&mut buf).unwrap();

    Ok(buf)
}

pub async fn unpack_from_reader<M, R>(reader: &mut R) -> Result<M, io::Error>
where
    M: Message + Default,
    R: AsyncReadExt + Unpin,
{
    let mut length_prefix = [0u8; MAX_MSG_LEN_BYTES];
    reader.read_exact(&mut length_prefix).await?;
    let data_length = u16::from_be_bytes(length_prefix) as usize;

    let mut data = vec![0u8; data_length];
    reader.read_exact(&mut data).await?;
    M::decode(Bytes::from(data)).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use futures::io::BufReader;

    use crate::proto::{dispersal, pack_message, unpack_from_reader};

    #[tokio::test]
    async fn test_pack_and_unpack_from_reader() {
        let blob = dispersal::Blob {
            blob_id: vec![0; 32],
            data: vec![1; 32],
        };
        let req = dispersal::DispersalReq { blob: Some(blob) };
        let message = dispersal::DispersalMessage {
            message_type: Some(dispersal::dispersal_message::MessageType::DispersalReq(req)),
        };

        let packed = pack_message(&message).unwrap();

        let mut reader = BufReader::new(&packed[..]);
        let unpacked: dispersal::DispersalMessage = unpack_from_reader(&mut reader).await.unwrap();

        assert_eq!(message, unpacked);
    }
}
