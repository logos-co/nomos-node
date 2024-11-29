use ark_bls12_381::Fr;
use ark_poly::{EvaluationDomain, GeneralEvaluationDomain};
use kzgrs::common::bytes_to_polynomial;
use kzgrs::rs::*;

use divan::counter::BytesCount;
use divan::{black_box, Bencher};
use rand::{thread_rng, RngCore};

fn main() {
    divan::main()
}

#[divan::bench(args = [3224])]
fn rs_encode(bencher: Bencher, size: usize) {
    bencher
        .with_inputs(move || {
            let mut buffer = vec![0u8; size];
            thread_rng().fill_bytes(&mut buffer);
            buffer
        })
        .input_counter(move |buff| BytesCount::new(buff.len()))
        .bench_refs(|buff| {
            let domain = GeneralEvaluationDomain::<Fr>::new(size).unwrap();
            let (_, poly) = bytes_to_polynomial::<31>(buff, domain).unwrap();
            let domain = GeneralEvaluationDomain::<Fr>::new(size * 2).unwrap();
            black_box(move || encode(&poly, domain))
        })
}

#[divan::bench(args = [3224], sample_size = 10, sample_count = 100)]
fn rs_decode(bencher: Bencher, size: usize) {
    bencher
        .with_inputs(move || {
            let mut buffer = vec![0u8; size];
            thread_rng().fill_bytes(&mut buffer);
            let domain = GeneralEvaluationDomain::<Fr>::new(size).unwrap();
            let (_, poly) = bytes_to_polynomial::<31>(&buffer, domain).unwrap();
            let domain = GeneralEvaluationDomain::<Fr>::new(size * 2).unwrap();
            let encoded = encode(&poly, domain);
            encoded
        })
        .input_counter(move |_buff| BytesCount::new(size * 31))
        .bench_values(|buff| {
            black_box(move || {
                let domain = GeneralEvaluationDomain::<Fr>::new(size).unwrap();
                let missing_data: Vec<_> = std::iter::repeat(None)
                    .take(size)
                    .chain(buff.evals[size..].iter().copied().map(Some))
                    .collect();
                decode(size, &missing_data, domain)
            })
        })
}
