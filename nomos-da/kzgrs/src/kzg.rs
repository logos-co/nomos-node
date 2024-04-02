use crate::common::KzgRsError;
use ark_bls12_381::{Bls12_381, Fr};
use ark_ec::pairing::Pairing;
use ark_poly::univariate::DensePolynomial;
use ark_poly::{DenseUVPolynomial, EvaluationDomain, GeneralEvaluationDomain, Polynomial};
use ark_poly_commit::kzg10::{Commitment, Powers, Proof, UniversalParams, KZG10};
use num_traits::One;
use std::ops::{Mul, Neg};

/// Commit to a polynomial where each of the evaluations are over `w(i)` for the degree
/// of the polynomial being omega (`w`) the root of unity (2^x).
pub fn commit_polynomial(
    polynomial: &DensePolynomial<Fr>,
    roots_of_unity: &Powers<Bls12_381>,
) -> Result<Commitment<Bls12_381>, KzgRsError> {
    KZG10::commit(roots_of_unity, polynomial, None, None)
        .map_err(KzgRsError::PolyCommitError)
        .map(|(commitment, _)| commitment)
}

/// Compute a witness polynomial in that satisfies `witness(x) = (f(x)-v)/(x-u)`
pub fn generate_element_proof(
    element_index: usize,
    polynomial: &DensePolynomial<Fr>,
    roots_of_unity: &Powers<Bls12_381>,
    domain: &GeneralEvaluationDomain<Fr>,
) -> Result<Proof<Bls12_381>, KzgRsError> {
    let u = domain.element(element_index);
    let v = polynomial.evaluate(&u);
    let f_x_v = polynomial + &DensePolynomial::<Fr>::from_coefficients_vec(vec![-v]);
    let x_u = DensePolynomial::<Fr>::from_coefficients_vec(vec![-u, Fr::one()]);
    let witness_polynomial: DensePolynomial<_> = &f_x_v / &x_u;
    let proof = commit_polynomial(&witness_polynomial, roots_of_unity)?;
    let proof = Proof {
        w: proof.0,
        random_v: None,
    };
    Ok(proof)
}

pub fn verify_element_proof(
    element_index: usize,
    element: &Fr,
    commitment: &Commitment<Bls12_381>,
    proof: &Proof<Bls12_381>,
    domain: &GeneralEvaluationDomain<Fr>,
    global_parameters: &UniversalParams<Bls12_381>,
) -> bool {
    let u = domain.element(element_index);
    let v = element;
    let commitment_check_g1 = commitment.0 + global_parameters.powers_of_g[0].mul(v).neg();
    let proof_check_g2 = global_parameters.beta_h + global_parameters.h.mul(u).neg();
    let lhs = Bls12_381::pairing(commitment_check_g1, global_parameters.h);
    let rhs = Bls12_381::pairing(proof.w, proof_check_g2);
    lhs == rhs
}

#[cfg(test)]
mod test {
    use crate::kzg::commit_polynomial;
    use ark_bls12_381::{Bls12_381, Fr};
    use ark_poly::univariate::DensePolynomial;
    use ark_poly::DenseUVPolynomial;
    use ark_poly_commit::kzg10::{Powers, KZG10};
    use std::borrow::Cow;

    #[test]
    fn test_poly_commit() {
        const COEFFICIENTS_SIZE: usize = 10;
        let mut rng = rand::thread_rng();
        let trusted_setup =
            KZG10::<Bls12_381, DensePolynomial<Fr>>::setup(COEFFICIENTS_SIZE - 1, true, &mut rng)
                .unwrap();

        let roots_of_unity = Powers {
            powers_of_g: Cow::Borrowed(&trusted_setup.powers_of_g),
            powers_of_gamma_g: Cow::Owned(
                (0..COEFFICIENTS_SIZE)
                    .map(|i| trusted_setup.powers_of_gamma_g[&i])
                    .collect(),
            ),
        };

        let poly = DensePolynomial::from_coefficients_vec((0..10).map(|i| Fr::from(i)).collect());
        let _ = commit_polynomial(&poly, &roots_of_unity).unwrap();
    }
}
