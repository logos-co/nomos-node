//! Custom variant of rust-kzg that supports dynamic sized blobs.
//! https://github.com/sifraitech/rust-kzg
//! Some types were changed to fit our API for comfort.
//! Blob related constants were removed and we use a config based approach.

use crate::types::{Blob, Commitment, KzgSettings, Proof};
use kzg::kzg_proofs::g1_linear_combination;
use kzg::types::fr::FsFr;
use kzg::types::g1::FsG1;
use kzg::types::kzg_settings::FsKZGSettings;
use kzg::types::poly::FsPoly;
use kzg_traits::eip_4844::{
    bytes_of_uint64, hash, hash_to_bls_field, verify_kzg_proof_rust, CHALLENGE_INPUT_SIZE,
    FIAT_SHAMIR_PROTOCOL_DOMAIN,
};
use kzg_traits::{Fr, Poly, G1};

pub fn blob_to_kzg_commitment(
    blob: &Blob,
    s: &FsKZGSettings,
    field_elements_per_blob: usize,
) -> FsG1 {
    let mut out = FsG1::default();
    g1_linear_combination(&mut out, &s.secret_g1, &blob.inner, field_elements_per_blob);
    out
}

pub fn compute_blob_kzg_proof(
    blob: &Blob,
    commitment: &Commitment,
    settings: &KzgSettings,
) -> Result<FsG1, String> {
    if !commitment.0.is_valid() {
        return Err("Invalid commitment".to_string());
    }

    let evaluation_challenge_fr = compute_challenge(blob, &commitment.0, settings);
    let (proof, _) = compute_kzg_proof(blob, &evaluation_challenge_fr, settings);
    Ok(proof)
}

pub fn verify_blob_kzg_proof(
    blob: &Blob,
    commitment: &Commitment,
    proof: &Proof,
    settings: &KzgSettings,
) -> Result<bool, String> {
    if !commitment.0.is_valid() {
        return Err("Invalid commitment".to_string());
    }
    if !proof.0.is_valid() {
        return Err("Invalid proof".to_string());
    }

    let polynomial = blob_to_polynomial(&blob.inner);
    let evaluation_challenge_fr = compute_challenge(blob, &commitment.0, settings);
    let y_fr = evaluate_polynomial_in_evaluation_form_rust(
        &polynomial,
        &evaluation_challenge_fr,
        &settings.settings,
    );
    verify_kzg_proof_rust(
        &commitment.0,
        &evaluation_challenge_fr,
        &y_fr,
        &proof.0,
        &settings.settings,
    )
}

fn compute_challenge(blob: &Blob, commitment: &FsG1, settings: &KzgSettings) -> FsFr {
    let mut bytes: Vec<u8> = vec![0; CHALLENGE_INPUT_SIZE];

    // Copy domain separator
    bytes[..16].copy_from_slice(&FIAT_SHAMIR_PROTOCOL_DOMAIN);
    bytes_of_uint64(&mut bytes[16..24], blob.len() as u64);
    // Set all other bytes of this 16-byte (little-endian) field to zero
    bytes_of_uint64(&mut bytes[24..32], 0);

    // Copy blob
    for i in 0..blob.len() {
        let v = blob.inner[i].to_bytes();
        bytes[(32 + i * settings.bytes_per_field_element)
            ..(32 + (i + 1) * settings.bytes_per_field_element)]
            .copy_from_slice(&v);
    }

    // Copy commitment
    let v = commitment.to_bytes();
    for i in 0..v.len() {
        bytes[32 + settings.bytes_per_field_element * blob.len() + i] = v[i];
    }

    // Now let's create the challenge!
    let eval_challenge = hash(&bytes);
    hash_to_bls_field(&eval_challenge)
}

fn compute_kzg_proof(blob: &Blob, z: &FsFr, s: &KzgSettings) -> (FsG1, FsFr) {
    let polynomial = blob_to_polynomial(blob.inner.as_slice());
    let poly_len = polynomial.coeffs.len();
    let y = evaluate_polynomial_in_evaluation_form_rust(&polynomial, z, &s.settings);

    let mut tmp: FsFr;
    let roots_of_unity: &Vec<FsFr> = &s.settings.fs.roots_of_unity;

    let mut m: usize = 0;
    let mut q: FsPoly = FsPoly::new(poly_len);

    let mut inverses_in: Vec<FsFr> = vec![FsFr::default(); poly_len];
    let mut inverses: Vec<FsFr> = vec![FsFr::default(); poly_len];

    for i in 0..poly_len {
        if z.equals(&roots_of_unity[i]) {
            // We are asked to compute a KZG proof inside the domain
            m = i + 1;
            inverses_in[i] = FsFr::one();
            continue;
        }
        // (p_i - y) / (ω_i - z)
        q.coeffs[i] = polynomial.coeffs[i].sub(&y);
        inverses_in[i] = roots_of_unity[i].sub(z);
    }

    fr_batch_inv(&mut inverses, &inverses_in, poly_len);

    for (i, inverse) in inverses.iter().enumerate().take(poly_len) {
        q.coeffs[i] = q.coeffs[i].mul(inverse);
    }

    if m != 0 {
        // ω_{m-1} == z
        m -= 1;
        q.coeffs[m] = FsFr::zero();
        for i in 0..poly_len {
            if i == m {
                continue;
            }
            // Build denominator: z * (z - ω_i)
            tmp = z.sub(&roots_of_unity[i]);
            inverses_in[i] = tmp.mul(z);
        }

        fr_batch_inv(&mut inverses, &inverses_in, poly_len);

        for i in 0..poly_len {
            if i == m {
                continue;
            }
            // Build numerator: ω_i * (p_i - y)
            tmp = polynomial.coeffs[i].sub(&y);
            tmp = tmp.mul(&roots_of_unity[i]);
            // Do the division: (p_i - y) * ω_i / (z * (z - ω_i))
            tmp = tmp.mul(&inverses[i]);
            q.coeffs[m] = q.coeffs[m].add(&tmp);
        }
    }

    let proof = g1_lincomb(&s.settings.secret_g1, &q.coeffs, poly_len);
    (proof, y)
}

fn evaluate_polynomial_in_evaluation_form_rust(p: &FsPoly, x: &FsFr, s: &FsKZGSettings) -> FsFr {
    let poly_len = p.coeffs.len();
    let roots_of_unity: &Vec<FsFr> = &s.fs.roots_of_unity;
    let mut inverses_in: Vec<FsFr> = vec![FsFr::default(); poly_len];
    let mut inverses: Vec<FsFr> = vec![FsFr::default(); poly_len];

    for i in 0..poly_len {
        if x.equals(&roots_of_unity[i]) {
            return p.get_coeff_at(i);
        }
        inverses_in[i] = x.sub(&roots_of_unity[i]);
    }

    fr_batch_inv(&mut inverses, &inverses_in, poly_len);

    let mut tmp: FsFr;
    let mut out = FsFr::zero();

    for i in 0..poly_len {
        tmp = inverses[i].mul(&roots_of_unity[i]);
        tmp = tmp.mul(&p.coeffs[i]);
        out = out.add(&tmp);
    }

    tmp = FsFr::from_u64(poly_len as u64);
    out = out.div(&tmp).unwrap();
    tmp = x.pow(poly_len);
    tmp = tmp.sub(&FsFr::one());
    out = out.mul(&tmp);
    out
}

fn fr_batch_inv(out: &mut [FsFr], a: &[FsFr], len: usize) {
    assert!(len > 0);

    let mut accumulator = FsFr::one();

    for i in 0..len {
        out[i] = accumulator;
        accumulator = accumulator.mul(&a[i]);
    }

    accumulator = accumulator.eucl_inverse();

    for i in (0..len).rev() {
        out[i] = out[i].mul(&accumulator);
        accumulator = accumulator.mul(&a[i]);
    }
}

fn g1_lincomb(points: &[FsG1], scalars: &[FsFr], length: usize) -> FsG1 {
    let mut out = FsG1::default();
    g1_linear_combination(&mut out, points, scalars, length);
    out
}

fn blob_to_polynomial(blob: &[FsFr]) -> FsPoly {
    let mut p: FsPoly = FsPoly::new(blob.len());
    p.coeffs = blob.to_vec();
    p
}

#[cfg(test)]
mod test {
    use super::*;
    use kzg::utils::generate_trusted_setup;
    use kzg_traits::{eip_4844::blob_to_kzg_commitment_rust, FFTSettings, KZGSettings};

    #[test]
    fn test_blob_to_kzg_commitment() {
        let (g1s, g2s) = generate_trusted_setup(4096, [0; 32]);
        let fft_settings = kzg::types::fft_settings::FsFFTSettings::new(8).unwrap();
        let settings = FsKZGSettings::new(&g1s, &g2s, 4096, &fft_settings).unwrap();
        let kzg_settings = KzgSettings {
            settings,
            bytes_per_field_element: 32,
        };
        let blob = Blob::from_bytes(&[5; 4096 * 32], &kzg_settings).unwrap();
        let commitment = blob_to_kzg_commitment(&blob, &kzg_settings.settings, 4096);
        let commitment2 = blob_to_kzg_commitment_rust(&blob.inner, &kzg_settings.settings).unwrap();
        assert_eq!(commitment, commitment2);
    }
}
