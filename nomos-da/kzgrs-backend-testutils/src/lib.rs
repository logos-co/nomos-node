// STD
// Crates
use kzgrs_backend::common::blob::DaBlob;
use kzgrs_backend::encoder;
use nomos_core::da::DaEncoder;
// Internal

const ENCODER_DOMAIN_SIZE: usize = 16;

pub fn get_encoder() -> encoder::DaEncoder {
    let params = encoder::DaEncoderParams::default_with(ENCODER_DOMAIN_SIZE);
    encoder::DaEncoder::new(params)
}

fn get_default_da_blob_data() -> Vec<u8> {
    vec![
        49u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
        0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8,
    ]
}

pub fn get_da_blob(data: Option<Vec<u8>>) -> DaBlob {
    let encoder = get_encoder();

    let data = data.unwrap_or_else(get_default_da_blob_data);
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
