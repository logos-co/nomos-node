use std::hint::black_box;

use divan::{counter::BytesCount, Bencher};
use kzgrs_backend::{
    common::blob::{DaBlobSharedCommitments, DaLightBlob},
    encoder::{DaEncoder, DaEncoderParams},
    global::GLOBAL_PARAMETERS,
    verifier::DaVerifier,
};
use nomos_core::da::DaEncoder as _;
use rand::{thread_rng, RngCore};

fn main() {
    divan::main();
}

const MB: usize = 1024;

#[must_use]
pub fn rand_data(elements_count: usize) -> Vec<u8> {
    let mut buff = vec![0; elements_count * DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE];
    thread_rng().fill_bytes(&mut buff);
    buff
}
#[divan::bench(consts = [32, 64, 128, 256, 512, 1024], args = [128, 256, 512, 1024, 2048, 4096], sample_count = 1, sample_size = 30)]
fn verify<const SIZE: usize>(bencher: Bencher, column_size: usize) {
    bencher
        .with_inputs(|| {
            let params = DaEncoderParams::new(column_size, true, GLOBAL_PARAMETERS.clone());

            let encoder = DaEncoder::new(params);
            let data = rand_data(SIZE * MB / DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE);
            let encoded_data = encoder.encode(&data).unwrap();
            let verifier = DaVerifier::new(GLOBAL_PARAMETERS.clone());
            let da_blob = DaLightBlob {
                column: encoded_data.extended_data.columns().next().unwrap(),
                column_idx: 0,
                column_commitment: encoded_data.column_commitments.first().copied().unwrap(),
                aggregated_column_proof: encoded_data
                    .aggregated_column_proofs
                    .first()
                    .copied()
                    .unwrap(),
                rows_proofs: encoded_data
                    .rows_proofs
                    .iter()
                    .map(|row| row.iter().next().copied().unwrap())
                    .collect(),
            };
            let commitments = DaBlobSharedCommitments {
                aggregated_column_commitment: encoded_data.aggregated_column_commitment,
                rows_commitments: encoded_data.row_commitments,
            };
            (verifier, commitments, da_blob)
        })
        .input_counter(|_| BytesCount::new(SIZE))
        .bench_refs(|(verifier, commitments, blob)| {
            black_box(verifier.verify(commitments, blob, column_size))
        });
}
