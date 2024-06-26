use divan::counter::BytesCount;
use divan::Bencher;
use kzgrs_backend::encoder::{DaEncoder, DaEncoderParams};
use once_cell::sync::Lazy;
use rand::RngCore;
use std::hint::black_box;

fn main() {
    divan::main()
}

static ENCODER: Lazy<DaEncoder> = Lazy::new(|| {
    let params = DaEncoderParams::new(4096, true);
    DaEncoder::new(params)
});

const KB: usize = 1024;

pub fn rand_data(elements_count: usize) -> Vec<u8> {
    let mut buff = vec![0; elements_count * DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE];
    rand::thread_rng().fill_bytes(&mut buff);
    buff
}
#[divan::bench(args = [32, 64, 128, 256, 512, 1024, 2048], sample_count = 1, sample_size = 1)]
fn encode(bencher: Bencher, size: usize) {
    bencher
        .with_inputs(|| rand_data(size * KB / DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE))
        .input_counter(|buff| BytesCount::new(buff.len()))
        .bench_refs(|buff| black_box(ENCODER.encode(buff)));
}
