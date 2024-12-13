// STD
use std::io;

// Crates
use futures::AsyncReadExt;
use nomos_core::wire;
use serde::de::DeserializeOwned;
use serde::Serialize;

// Internal
use crate::Result;

type LenType = u16;
const MAX_MSG_LEN_BYTES: usize = size_of::<LenType>();
const MAX_MSG_LEN: usize = 1 << (MAX_MSG_LEN_BYTES * 8);

fn into_failed_to_serialize(error: wire::Error) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("Failed to serialize message: {}", error),
    )
}

fn into_failed_to_deserialize(error: wire::Error) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("Failed to deserialize message: {}", error),
    )
}

fn get_message_too_large_error(message_len: usize) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "Message too large. Maximum size is {}. Actual size is {}",
            MAX_MSG_LEN, message_len
        ),
    )
}

pub fn pack<Message>(message: &Message) -> Result<Vec<u8>>
where
    Message: Serialize,
{
    let serialized_message = wire::serialize(message).map_err(into_failed_to_serialize)?;

    let data_length = serialized_message.len();
    if serialized_message.len() > MAX_MSG_LEN {
        return Err(get_message_too_large_error(data_length));
    }

    let mut buffer = Vec::with_capacity(MAX_MSG_LEN_BYTES + data_length);
    buffer.extend_from_slice(&(data_length as u16).to_be_bytes());
    buffer.extend_from_slice(&serialized_message);
    Ok(buffer)
}

async fn read_data_length<R>(reader: &mut R) -> Result<usize>
where
    R: AsyncReadExt + Unpin,
{
    let mut length_prefix = [0u8; MAX_MSG_LEN_BYTES];
    reader.read_exact(&mut length_prefix).await?;
    Ok(u16::from_be_bytes(length_prefix) as usize)
}

pub async fn unpack_from_reader<Message, R>(reader: &mut R) -> Result<Message>
where
    Message: DeserializeOwned,
    R: AsyncReadExt + Unpin,
{
    let data_length = read_data_length(reader).await?;
    let mut data = vec![0u8; data_length];
    reader.read_exact(&mut data).await?;
    wire::deserialize(&data).map_err(into_failed_to_deserialize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::Blob;
    use crate::dispersal::{DispersalError, DispersalErrorType, DispersalRequest};
    use futures::io::BufReader;
    use kzgrs_backend::common::blob::DaBlob;
    use kzgrs_backend::encoder::{self, DaEncoderParams};
    use nomos_core::da::{BlobId, DaEncoder};

    fn get_encoder() -> encoder::DaEncoder {
        const DOMAIN_SIZE: usize = 16;
        let params = DaEncoderParams::default_with(DOMAIN_SIZE);
        encoder::DaEncoder::new(params)
    }

    fn get_da_blob() -> DaBlob {
        let encoder = get_encoder();
        let data = vec![
            49u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
            0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        ];

        let encoded_data = encoder.encode(&data).unwrap();
        let columns: Vec<_> = encoded_data.extended_data.columns().collect();

        let index = 0;
        let da_blob = DaBlob {
            column: columns[index].clone(),
            column_idx: index
                .try_into()
                .expect("Column index shouldn't overflow the target type"),
            column_commitment: encoded_data.column_commitments[index],
            aggregated_column_commitment: encoded_data.aggregated_column_commitment,
            aggregated_column_proof: encoded_data.aggregated_column_proofs[index],
            rows_commitments: encoded_data.row_commitments.clone(),
            rows_proofs: encoded_data
                .rows_proofs
                .iter()
                .map(|proofs| proofs.get(index).cloned().unwrap())
                .collect(),
        };

        da_blob
    }

    #[tokio::test]
    async fn pack_and_unpack_from_reader() -> Result<()> {
        let blob_id = BlobId::from([0; 32]);
        let data = get_da_blob();
        let blob = Blob::new(blob_id, data);
        let subnetwork_id = 0;
        let message = DispersalRequest::new(blob, subnetwork_id);

        let packed_message = pack(&message)?;

        let mut reader = BufReader::new(packed_message.as_ref());
        let unpacked_message: DispersalRequest = unpack_from_reader(&mut reader).await?;

        assert_eq!(message, unpacked_message);
        Ok(())
    }

    #[tokio::test]
    async fn packing_too_large_message() {
        let blob_id = BlobId::from([0; 32]);
        let error_description = [". "; MAX_MSG_LEN].concat();
        let message =
            DispersalError::new(blob_id, DispersalErrorType::ChunkSize, error_description);

        let packed_message = pack(&message);
        assert!(packed_message.is_err());
        assert_eq!(
            packed_message.unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
