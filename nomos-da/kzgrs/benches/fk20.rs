use ark_bls12_381::{Bls12_381, Fr, G1Affine, G1Projective};
use ark_ec::AffineRepr;
use ark_ff::BigInt;
use ark_poly::univariate::DensePolynomial;
use ark_poly::{EvaluationDomain, GeneralEvaluationDomain};
use ark_poly_commit::kzg10::KZG10;
use divan::counter::ItemsCount;
use divan::Bencher;
use kzgrs::fk20::fk20_batch_generate_elements_proofs;
use kzgrs::{bytes_to_polynomial, GlobalParameters, BYTES_PER_FIELD_ELEMENT};
use once_cell::sync::Lazy;
use rand::SeedableRng;
#[cfg(feature = "parallel")]
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::hint::black_box;

fn main() {
    divan::main()
}

static GLOBAL_PARAMETERS: Lazy<GlobalParameters> = Lazy::new(|| {
    let mut rng = rand::rngs::StdRng::seed_from_u64(1987);
    KZG10::<Bls12_381, DensePolynomial<Fr>>::setup(4096, true, &mut rng).unwrap()
});

#[divan::bench(args = [16, 32, 64, 128, 256, 512, 1024, 2048, 4096])]
fn compute_fk20_proofs_for_size(bencher: Bencher, size: usize) {
    bencher
        .with_inputs(|| {
            let buff: Vec<_> = (0..BYTES_PER_FIELD_ELEMENT * size)
                .map(|i| (i % 255) as u8)
                .rev()
                .collect();
            let domain = GeneralEvaluationDomain::new(size).unwrap();
            let (_, poly) = bytes_to_polynomial::<BYTES_PER_FIELD_ELEMENT>(&buff, domain).unwrap();
            poly
        })
        .input_counter(move |_| ItemsCount::new(size))
        .bench_refs(|(poly)| {
            black_box(fk20_batch_generate_elements_proofs(
                poly,
                &GLOBAL_PARAMETERS,
            ))
        });
}

#[cfg(feature = "parallel")]
#[divan::bench(args = [16, 32, 64, 128, 256, 512, 1024, 2048, 4096])]
fn compute_parallel_fk20_proofs_for_size(bencher: Bencher, size: usize) {
    const THREAD_COUNT: usize = 8;
    bencher
        .with_inputs(|| {
            let buff: Vec<_> = (0..BYTES_PER_FIELD_ELEMENT * size)
                .map(|i| (i % 255) as u8)
                .rev()
                .collect();
            let domain = GeneralEvaluationDomain::new(size).unwrap();
            let (_, poly) = bytes_to_polynomial::<BYTES_PER_FIELD_ELEMENT>(&buff, domain).unwrap();
            poly
        })
        .input_counter(move |_| ItemsCount::new(size * THREAD_COUNT))
        .bench_refs(|(poly)| {
            black_box((0..THREAD_COUNT).into_par_iter().for_each(|_| {
                fk20_batch_generate_elements_proofs(poly, &GLOBAL_PARAMETERS);
            }))
        });
}
