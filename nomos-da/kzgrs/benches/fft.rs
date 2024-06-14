use ark_bls12_381::{Fr, G1Affine};
use ark_ec::{AffineRepr, CurveGroup};
use ark_ff::{BigInt, FftField, Field};
use divan::counter::ItemsCount;
use divan::{black_box, counter::BytesCount, AllocProfiler, Bencher};
use kzgrs::fft::{fft_g1, ifft_g1};
fn main() {
    divan::main()
}

#[divan::bench(args = [16, 32, 64, 128, 256, 512, 1024, 2048, 4096])]
fn compute_fft_for_size(bencher: Bencher, size: usize) {
    bencher
        .with_inputs(|| {
            let primitive_root = <Fr as FftField>::get_root_of_unity(size as u64).unwrap();
            let roots_of_unity: Vec<_> = (1..=size)
                .map(|i| primitive_root.pow::<ark_ff::BigInt<4>>(BigInt::from(i as u64)))
                .collect();
            let buff: Vec<G1Affine> = (0..size)
                .map(|i| {
                    G1Affine::identity()
                        .mul_bigint(BigInt::<4>::from(i as u64))
                        .into_affine()
                })
                .collect();
            (buff, roots_of_unity)
        })
        .input_counter(move |_| ItemsCount::new(size))
        .bench_refs(|(buff, roots_of_unity)| black_box(fft_g1(buff, roots_of_unity)));
}

#[divan::bench(args = [16, 32, 64, 128, 256, 512, 1024, 2048, 4096])]
fn compute_ifft_for_size(bencher: Bencher, size: usize) {
    bencher
        .with_inputs(|| {
            let primitive_root = <Fr as FftField>::get_root_of_unity(size as u64).unwrap();
            let roots_of_unity: Vec<_> = (1..=size)
                .map(|i| primitive_root.pow::<ark_ff::BigInt<4>>(BigInt::from(i as u64)))
                .collect();
            let buff: Vec<G1Affine> = (0..size)
                .map(|i| {
                    G1Affine::identity()
                        .mul_bigint(BigInt::<4>::from(i as u64))
                        .into_affine()
                })
                .collect();
            let buff = fft_g1(&buff, &roots_of_unity);
            (buff, roots_of_unity)
        })
        .input_counter(move |_| ItemsCount::new(size))
        .bench_refs(|(buff, roots_of_unity)| black_box(ifft_g1(buff, roots_of_unity)));
}
