use crate::common::blob::DaBlob;
use crate::common::Chunk;
use kzgrs::BYTES_PER_FIELD_ELEMENT;

/// Reconstruct original data from a set of `DaBlob`
/// Warning! This does not interpolate so it should not be used on blobs which doesn't represent
/// the original set of data.
pub fn reconstruct_without_missing_data(blobs: &[DaBlob]) -> Vec<u8> {
    // pick positions from columns
    let mut data: Vec<((usize, usize), Vec<u8>)> = blobs
        .iter()
        .flat_map(|blob| {
            blob.column
                .iter()
                .map(Chunk::as_bytes)
                .enumerate()
                .map(|(row, data)| ((row, blob.column_idx as usize), data.to_vec()))
        })
        .collect();
    data.sort_unstable_by_key(|(k, _)| *k);
    data.into_iter()
        .flat_map(|(_, mut data)| {
            assert_eq!(data.len(), BYTES_PER_FIELD_ELEMENT);
            // pop last byte as we fill it empty
            data.pop();
            data.into_iter()
        })
        .collect()
}

#[cfg(test)]
mod test {
    use crate::common::blob::DaBlob;
    use crate::common::ColumnIndex;
    use crate::encoder::test::rand_data;
    use crate::encoder::{DaEncoder, DaEncoderParams, EncodedData};
    use crate::reconstruction::reconstruct_without_missing_data;
    use nomos_core::da::DaEncoder as _;

    #[test]
    fn test_reconstruct() {
        let data: Vec<u8> = rand_data(32);
        let params = DaEncoderParams::default_with(4);
        let encoder = DaEncoder::new(params);
        let encoded_data: EncodedData = encoder.encode(&data).unwrap();
        let blobs: Vec<DaBlob> = encoded_data
            .chunked_data
            .columns()
            .enumerate()
            .map(|(idx, column)| DaBlob {
                column,
                column_idx: idx as ColumnIndex,
                column_commitment: Default::default(),
                aggregated_column_commitment: Default::default(),
                aggregated_column_proof: Default::default(),
                rows_commitments: vec![],
                rows_proofs: vec![],
            })
            .collect();
        assert_eq!(data, reconstruct_without_missing_data(&blobs));
    }
}
