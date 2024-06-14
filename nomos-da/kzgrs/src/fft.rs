use ark_bls12_381::{Bls12_381, Fr, G1Affine};
use ark_ec::pairing::Pairing;
use ark_ec::{AffineRepr, CurveGroup};
use ark_ff::{BigInt, BigInteger, FftField, Field, PrimeField};
#[cfg(feature = "parallel")]
use rayon::iter::IntoParallelIterator;

pub fn fft_g1(vals: &[G1Affine], roots_of_unity: &[Fr]) -> Vec<G1Affine> {
    debug_assert_eq!(vals.len(), roots_of_unity.len());
    if vals.len() == 1 {
        return vals.to_vec();
    }
    let half_roots: Vec<_> = roots_of_unity.iter().step_by(2).copied().collect();

    let l = || {
        fft_g1(
            vals.iter()
                .step_by(2)
                .copied()
                .collect::<Vec<_>>()
                .as_slice(),
            half_roots.as_slice(),
        )
    };

    let r = || {
        fft_g1(
            vals.iter()
                .skip(1)
                .step_by(2)
                .copied()
                .collect::<Vec<_>>()
                .as_slice(),
            half_roots.as_slice(),
        )
    };

    let [l, r]: [Vec<G1Affine>; 2] = {
        #[cfg(feature = "parallel")]
        {
            [l, r].into_par_iter().map(|f| f()).collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            [l(), r()]
        }
    };

    let y_times_root = {
        #[cfg(feature = "parallel")]
        {
            r.into_par_iter()
        }
        #[cfg(not(feature = "parallel"))]
        {
            r.into_iter()
        }
    }
    .cycle()
    .enumerate()
    .map(|(i, y)| (y * roots_of_unity[i % vals.len()]).into_affine());

    {
        #[cfg(feature = "parallel")]
        {
            l.into_par_iter()
        }
        #[cfg(not(feature = "parallel"))]
        {
            l.into_iter()
        }
    }
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

pub fn ifft_g1(vals: &[G1Affine], roots_of_unity: &[Fr]) -> Vec<G1Affine> {
    debug_assert_eq!(vals.len(), roots_of_unity.len());
    let mut mod_min_2 = BigInt::new(<Fr as PrimeField>::MODULUS.0);
    mod_min_2.sub_with_borrow(&BigInt::<4>::from(2u64));
    let invlen = Fr::from(vals.len() as u64).pow(mod_min_2).into_bigint();
    #[cfg(feature = "parallel")]
    {
        fft_g1(vals, roots_of_unity).into_par_iter().collect()
    }
    #[cfg(not(feature = "parallel"))]
    { fft_g1(vals, roots_of_unity).into_iter() }
        .map(|g| g.mul_bigint(invlen).into_affine())
        .collect()
}

#[cfg(test)]
mod test {
    use crate::fft::{fft_g1, ifft_g1};
    use ark_bls12_381::{Fr, G1Affine};
    use ark_ec::{AffineRepr, CurveGroup};
    use ark_ff::{BigInt, FftField, Field};

    #[test]
    fn test_fft_ifft_g1() {
        for size in [16usize, 32, 64, 128, 256, 512, 1024, 2048, 4096] {
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
            let fft = fft_g1(&buff, &roots_of_unity);
            let ifft = ifft_g1(&fft, &roots_of_unity);
            assert_eq!(buff, ifft);
        }
    }
}
