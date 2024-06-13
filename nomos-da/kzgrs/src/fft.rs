use ark_bls12_381::{Fr, G1Affine};
use ark_ec::CurveGroup;

pub fn fft_g1(vals: &[G1Affine], roots_of_unity: &[Fr]) -> Vec<G1Affine> {
    if vals.len() == 1 {
        return vals.to_vec();
    }
    let half_roots: Vec<_> = roots_of_unity.iter().step_by(2).copied().collect();

    let l = fft_g1(
        vals.iter()
            .step_by(2)
            .copied()
            .collect::<Vec<_>>()
            .as_slice(),
        half_roots.as_slice(),
    );

    let r = fft_g1(
        vals.iter()
            .skip(1)
            .step_by(2)
            .copied()
            .collect::<Vec<_>>()
            .as_slice(),
        half_roots.as_slice(),
    );

    let y_times_root = r
        .into_iter()
        .cycle()
        .enumerate()
        .map(|(i, y)| (y * roots_of_unity[i % vals.len()]).into_affine());

    l.into_iter()
        .cycle()
        .take(vals.len())
        .zip(y_times_root)
        .enumerate()
        .map(|(i, (x, y_times_root))| {
            if i < vals.len() / 2 {
                x + y_times_root
            } else {
                x - y_times_root
            }
            .into_affine()
        })
        .collect()
}
