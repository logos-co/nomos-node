use blst::min_sig::SecretKey;
use divan::counter::BytesCount;
use divan::Bencher;
use kzgrs_backend::common::blob::DaBlob;
use kzgrs_backend::encoder::{DaEncoder, DaEncoderParams};
use kzgrs_backend::global::GLOBAL_PARAMETERS;
use kzgrs_backend::verifier::DaVerifier;
use nomos_core::da::DaEncoder as _;
use rand::{thread_rng, RngCore};
use std::hint::black_box;

fn main() {
    divan::main()
}

const MB: usize = 1024;

pub fn rand_data(elements_count: usize) -> Vec<u8> {
    let mut buff = vec![0; elements_count * DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE];
    rand::thread_rng().fill_bytes(&mut buff);
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
            let mut buff = [0u8; 32];
            let mut rng = thread_rng();
            rng.fill_bytes(&mut buff);
            let sk = SecretKey::key_gen(&buff, &[]).unwrap();
            let verifier = DaVerifier::new(
                sk.clone(),
                (0..column_size as u16).collect(),
                GLOBAL_PARAMETERS.clone(),
            );
            let da_blob = DaBlob {
                column: encoded_data.extended_data.columns().next().unwrap(),
                column_idx: 0,
                column_commitment: encoded_data
                    .column_commitments
                    .iter()
                    .next()
                    .copied()
                    .unwrap(),
                aggregated_column_commitment: encoded_data.aggregated_column_commitment.clone(),

                aggregated_column_proof: encoded_data
                    .aggregated_column_proofs
                    .iter()
                    .next()
                    .copied()
                    .unwrap(),
                rows_commitments: encoded_data.row_commitments.clone(),
                rows_proofs: encoded_data
                    .rows_proofs
                    .iter()
                    .map(|row| row.iter().next().copied().unwrap())
                    .collect(),
            };
            (verifier, da_blob)
        })
        .input_counter(|_| BytesCount::new(SIZE))
        .bench_refs(|(verifier, blob)| black_box(verifier.verify(blob, column_size)));
}
