// STD
use futures::{AsyncReadExt, AsyncWriteExt};
use std::io;
// Crates
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

struct MessageTooLargeError(usize);

impl From<MessageTooLargeError> for io::Error {
    fn from(value: MessageTooLargeError) -> Self {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Message too large. Maximum size is {}. Actual size is {}",
                MAX_MSG_LEN, value.0
            ),
        )
    }
}

pub fn pack<Message>(message: &Message) -> Result<Vec<u8>>
where
    Message: Serialize,
{
    wire::serialize(message).map_err(into_failed_to_serialize)
}

fn get_packed_message_size(packed_message: &[u8]) -> Result<usize> {
    let data_length = packed_message.len();
    if data_length > MAX_MSG_LEN {
        return Err(MessageTooLargeError(data_length).into());
    }
    Ok(data_length)
}

fn prepare_message_for_writer(packed_message: &[u8]) -> Result<Vec<u8>> {
    let data_length = get_packed_message_size(packed_message)?;
    let mut buffer = Vec::with_capacity(MAX_MSG_LEN_BYTES + data_length);
    buffer.extend_from_slice(&(data_length as LenType).to_be_bytes());
    buffer.extend_from_slice(packed_message);
    Ok(buffer)
}

pub async fn pack_to_writer<Message, Writer>(message: &Message, writer: &mut Writer) -> Result<()>
where
    Message: Serialize,
    Writer: AsyncWriteExt + Unpin,
{
    let packed_message = pack(message)?;
    let prepared_packed_message = prepare_message_for_writer(&packed_message)?;
    writer.write_all(&prepared_packed_message).await
}

async fn read_data_length<R>(reader: &mut R) -> Result<usize>
where
    R: AsyncReadExt + Unpin,
{
    let mut length_prefix = [0u8; MAX_MSG_LEN_BYTES];
    reader.read_exact(&mut length_prefix).await?;
    let s = LenType::from_be_bytes(length_prefix) as usize;
    Ok(s)
}

pub fn unpack<M: DeserializeOwned>(data: &[u8]) -> Result<M> {
    wire::deserialize(data).map_err(into_failed_to_deserialize)
}

pub async fn unpack_from_reader<Message, R>(reader: &mut R) -> Result<Message>
where
    Message: DeserializeOwned,
    R: AsyncReadExt + Unpin,
{
    let data_length = read_data_length(reader).await?;
    let mut data = vec![0u8; data_length];
    reader.read_exact(&mut data).await?;
    unpack(&data)
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
    async fn pack_and_unpack() -> Result<()> {
        let blob_id = BlobId::from([0; 32]);
        let data = get_da_blob();
        let blob = Blob::new(blob_id, data);
        let subnetwork_id = 0;
        let message = DispersalRequest::new(blob, subnetwork_id);

        let packed_message = pack(&message)?;
        let unpacked_message: DispersalRequest = unpack(&packed_message)?;

        assert_eq!(message, unpacked_message);
        Ok(())
    }

    #[tokio::test]
    async fn pack_to_writer_and_unpack_from_reader() -> Result<()> {
        let blob_id = BlobId::from([0; 32]);
        let data = get_da_blob();
        let blob = Blob::new(blob_id, data);
        let subnetwork_id = 0;
        let message = DispersalRequest::new(blob, subnetwork_id);

        let mut writer = Vec::new();
        pack_to_writer(&message, &mut writer).await?;

        let mut reader = BufReader::new(writer.as_slice());
        let unpacked_message: DispersalRequest = unpack_from_reader(&mut reader).await?;

        assert_eq!(message, unpacked_message);
        Ok(())
    }

    #[tokio::test]
    async fn pack_to_writer_too_large_message() {
        let blob_id = BlobId::from([0; 32]);
        let error_description = ["."; MAX_MSG_LEN].concat();
        let message =
            DispersalError::new(blob_id, DispersalErrorType::ChunkSize, error_description);

        let mut writer = Vec::new();
        let res = pack_to_writer(&message, &mut writer).await;
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }
}
