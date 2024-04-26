use divan::counter::ItemsCount;
use divan::{black_box, counter::BytesCount, AllocProfiler, Bencher};
use kzgrs::{Evaluations, Polynomial};
use rand::RngCore;

use kzgrs_backend::encoder::{DaEncoder, DaEncoderParams};
fn main() {
    divan::main()
}

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();
const SIZES: &[usize] = &[1usize, 2, 3, 4, 5, 6, 7, 8];

fn rand_data(size_mb: usize) -> Vec<u8> {
    let elements_count = size_mb * 1024 * 1024 / DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE;
    let mut buff = vec![0u8; elements_count * DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE];
    rand::thread_rng().fill_bytes(&mut buff);
    buff
}

#[allow(non_snake_case)]
#[divan::bench(args = SIZES)]
fn chunkify_MB(bencher: Bencher, size: usize) {
    let encoder_params: DaEncoderParams = DaEncoderParams::default_with(10);
    let encoder: DaEncoder = DaEncoder::new(encoder_params);
    bencher
        .with_inputs(|| rand_data(size))
        .input_counter(BytesCount::of_slice)
        .bench_refs(|data| black_box(encoder.chunkify(data)));
}

#[allow(non_snake_case)]
#[divan::bench(args = [100, 1000, 10000], sample_count = 10)]
fn compute_1MB_data_single_kzg_row_commitments_with_column_count(
    bencher: Bencher,
    column_count: usize,
) {
    let encoder_params: DaEncoderParams = DaEncoderParams::default_with(column_count);
    let encoder: DaEncoder = DaEncoder::new(encoder_params);
    let size = bencher
        .with_inputs(|| encoder.chunkify(rand_data(1).as_ref()))
        .input_counter(|matrix| BytesCount::of_slice(&matrix.0[0].as_bytes()))
        .bench_refs(|matrix| {
            black_box(DaEncoder::compute_kzg_row_commitment(&matrix.0[0]).is_ok())
        });
}

#[allow(non_snake_case)]
#[divan::bench(args = [100, 1000, 10000], sample_count = 1, sample_size = 1)]
fn compute_1MB_data_matrix_kzg_row_commitments_with_column_count(
    bencher: Bencher,
    column_count: usize,
) {
    let encoder_params: DaEncoderParams = DaEncoderParams::default_with(column_count);
    let encoder: DaEncoder = DaEncoder::new(encoder_params);
    let size = bencher
        .with_inputs(|| encoder.chunkify(rand_data(1).as_ref()))
        .input_counter(|matrix| BytesCount::new(matrix.bytes_size()))
        .bench_refs(|matrix| black_box(DaEncoder::compute_kzg_row_commitments(&matrix).is_ok()));
}

#[allow(non_snake_case)]
#[divan::bench(args = [100, 1000, 10000], sample_count = 1, sample_size = 1)]
fn compute_1MB_data_rs_encode_rows_with_column_count(bencher: Bencher, column_count: usize) {
    let encoder_params: DaEncoderParams = DaEncoderParams::default_with(column_count);
    let encoder: DaEncoder = DaEncoder::new(encoder_params);
    let size = bencher
        .with_inputs(|| {
            let matrix = encoder.chunkify(rand_data(1).as_ref());
            let (row_polynomials, _): (Vec<(Evaluations, Polynomial)>, Vec<_>) =
                DaEncoder::compute_kzg_row_commitments(&matrix)
                    .unwrap()
                    .into_iter()
                    .unzip();
            row_polynomials
        })
        .input_counter(|polynomials| ItemsCount::new(polynomials.len()))
        .bench_refs(|polynomials| black_box(DaEncoder::rs_encode_rows(polynomials)));
}

#[allow(non_snake_case)]
#[divan::bench(args = [100, 1000, 10000], sample_count = 1, sample_size = 1)]
fn compute_1MB_data_compute_rows_proofs_with_column_count(bencher: Bencher, column_count: usize) {
    let encoder_params: DaEncoderParams = DaEncoderParams::default_with(column_count);
    let encoder: DaEncoder = DaEncoder::new(encoder_params);
    let size = bencher
        .with_inputs(|| {
            let matrix = encoder.chunkify(rand_data(1).as_ref());
            let (row_polynomials, _): (Vec<(Evaluations, Polynomial)>, Vec<_>) =
                DaEncoder::compute_kzg_row_commitments(&matrix)
                    .unwrap()
                    .into_iter()
                    .unzip();
            row_polynomials
                .into_iter()
                .map(|(_, poly)| poly)
                .collect::<Vec<_>>()
        })
        .input_counter(|polynomials| ItemsCount::new(polynomials.len()))
        .bench_refs(|polynomials| {
            black_box(DaEncoder::compute_rows_proofs(polynomials, column_count).is_ok())
        });
}

#[allow(non_snake_case)]
#[divan::bench(args = [100, 1000, 10000], sample_count = 1, sample_size = 1)]
fn encode_1MB_with_column_count(bencher: Bencher, column_count: usize) {
    let encoder_params: DaEncoderParams = DaEncoderParams::default_with(column_count);
    let encoder: DaEncoder = DaEncoder::new(encoder_params);
    let size = bencher
        .with_inputs(|| rand_data(1))
        .input_counter(|data| BytesCount::of_slice(&data))
        .bench_refs(|data| black_box(encoder.encode(data).is_ok()));
}
