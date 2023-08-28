use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kzg::{types::kzg_settings::FsKZGSettings, utils::generate_trusted_setup};
use kzg_traits::{FFTSettings, KZGSettings};
use nomos_kzg::Blob;

fn nomos_dynamic_vs_external(c: &mut Criterion) {
    let (g1s, g2s) = generate_trusted_setup(4096, [0; 32]);
    let fft_settings = kzg::types::fft_settings::FsFFTSettings::new(8).unwrap();
    let settings = FsKZGSettings::new(&g1s, &g2s, 4096, &fft_settings).unwrap();
    let blob = Blob::from_bytes(&[5; 4096 * 32]).unwrap();

    let mut group = c.benchmark_group("KZG Commitment Benchmarks");

    group.bench_function("nomos blob commitment", |b| {
        b.iter(|| {
            nomos_kzg::blob_to_kzg_commitment(
                black_box(&blob),
                black_box(&settings),
                black_box(4096),
            )
        })
    });

    group.bench_function("external blob commitment", |b| {
        b.iter(|| {
            kzg::eip_4844::blob_to_kzg_commitment_rust(
                black_box(&blob.inner()),
                black_box(&settings),
            )
        })
    });

    group.finish();
}

criterion_group!(benches, nomos_dynamic_vs_external);
criterion_main!(benches);
