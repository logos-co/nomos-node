use crate::common::KzgRsError;
use crate::Evaluations;
use ark_bls12_381::{Bls12_381, Fr};
use ark_ec::pairing::Pairing;
use ark_ff::UniformRand;
use ark_poly::univariate::DensePolynomial;
use ark_poly::{DenseUVPolynomial, EvaluationDomain, GeneralEvaluationDomain};
use ark_poly_commit::kzg10::{Commitment, Powers, Proof, UniversalParams, KZG10};
use num_traits::{One, Zero};
use rand::rngs::OsRng;
use std::borrow::Cow;
use std::ops::{Mul, Neg};

const MIN_DOMAIN_SIZE: usize = 64;
const HIDING_BOUND: usize = 128;

/// Commit to a polynomial where each of the evaluations are over `w(i)` for the degree
/// of the polynomial being omega (`w`) the root of unity (2^x).
pub fn commit_polynomial(
    polynomial: &DensePolynomial<Fr>,
    global_parameters: &UniversalParams<Bls12_381>,
) -> Result<Commitment<Bls12_381>, KzgRsError> {
    let mut rng = OsRng;

    // Ensure that powers_of_gamma_g has enough powers for the hiding bound
    #[cfg(not(test))]
    if global_parameters.powers_of_gamma_g.len() < HIDING_BOUND + 1 {
        return Err(KzgRsError::HidingBoundTooLarge);
    }

    // Convert from B-tree map
    let powers_of_gamma_g: Vec<_> = global_parameters
        .powers_of_gamma_g
        .values()
        .cloned()
        .collect();

    // Initialize Powers with sufficient powers_of_gamma_g
    let roots_of_unity = Powers {
        powers_of_g: Cow::Borrowed(&global_parameters.powers_of_g),
        powers_of_gamma_g: Cow::Borrowed(&powers_of_gamma_g),
    };

    KZG10::commit(
        &roots_of_unity,
        polynomial,
        Some(HIDING_BOUND),
        Some(&mut rng),
    )
    .map_err(KzgRsError::PolyCommitError)
    .map(|(commitment, _)| commitment)
}

/// Compute a witness polynomial in that satisfies `witness(x) = (f(x)-v)/(x-u)`
pub fn generate_element_proof(
    element_index: usize,
    polynomial: &DensePolynomial<Fr>,
    evaluations: &Evaluations,
    global_parameters: &UniversalParams<Bls12_381>,
    domain: GeneralEvaluationDomain<Fr>,
) -> Result<Proof<Bls12_381>, KzgRsError> {
    let u = domain.element(element_index);
    if u.is_zero() {
        return Err(KzgRsError::DivisionByZeroPolynomial);
    };

    #[cfg(not(test))]
    if domain.size() < MIN_DOMAIN_SIZE {
        return Err(KzgRsError::DomainSizeTooSmall(domain.size()));
    };

    // Instead of evaluating over the polynomial, we can reuse the evaluation points from the rs encoding
    // let v = polynomial.evaluate(&u);
    let v = evaluations.evals[element_index];

    // Generate randomness for v (random_v)
    let mut rng = OsRng;
    let random_v = Fr::rand(&mut rng);

    // Compute the randomized v' = v + random_v
    let v_prime = v + random_v;

    // Compute f(x) - v' (adjusted for randomness)
    let f_x_v = polynomial + &DensePolynomial::<Fr>::from_coefficients_vec(vec![-v_prime]);

    // Compute x - u
    let x_u = DensePolynomial::<Fr>::from_coefficients_vec(vec![-u, Fr::one()]);

    // Compute the witness polynomial: (f(x) - v') / (x - u)
    let witness_polynomial: DensePolynomial<_> = &f_x_v / &x_u;
    let proof = commit_polynomial(&witness_polynomial, global_parameters)?;
    let proof = Proof {
        w: proof.0,
        random_v: Some(random_v),
    };
    Ok(proof)
}

/// Verify proof for a single element
pub fn verify_element_proof(
    element_index: usize,
    element: &Fr,
    commitment: &Commitment<Bls12_381>,
    proof: &Proof<Bls12_381>,
    domain: GeneralEvaluationDomain<Fr>,
    global_parameters: &UniversalParams<Bls12_381>,
) -> bool {
    let u = domain.element(element_index);
    let v = *element + proof.random_v.unwrap();
    let commitment_check_g1 = commitment.0 + global_parameters.powers_of_g[0].mul(v).neg();
    let proof_check_g2 = global_parameters.beta_h + global_parameters.h.mul(u).neg();
    let lhs = Bls12_381::pairing(commitment_check_g1, global_parameters.h);
    let rhs = Bls12_381::pairing(proof.w, proof_check_g2);
    lhs == rhs
}

#[cfg(test)]
mod test {
    use crate::common::bytes_to_polynomial;
    use crate::kzg::{commit_polynomial, generate_element_proof, verify_element_proof};
    use ark_bls12_381::{Bls12_381, Fr};
    use ark_poly::univariate::DensePolynomial;
    use ark_poly::{DenseUVPolynomial, EvaluationDomain, GeneralEvaluationDomain};
    use ark_poly_commit::kzg10::{UniversalParams, KZG10};
    use once_cell::sync::Lazy;
    use rand::{thread_rng, Fill};
    use rayon::iter::{IndexedParallelIterator, ParallelIterator};
    use rayon::prelude::IntoParallelRefIterator;

    const COEFFICIENTS_SIZE: usize = 16;
    static GLOBAL_PARAMETERS: Lazy<UniversalParams<Bls12_381>> = Lazy::new(|| {
        let mut rng = rand::thread_rng();
        KZG10::<Bls12_381, DensePolynomial<Fr>>::setup(
            crate::kzg::test::COEFFICIENTS_SIZE - 1,
            true,
            &mut rng,
        )
        .unwrap()
    });

    static DOMAIN: Lazy<GeneralEvaluationDomain<Fr>> =
        Lazy::new(|| GeneralEvaluationDomain::new(COEFFICIENTS_SIZE).unwrap());
    #[test]
    fn test_poly_commit() {
        let poly = DensePolynomial::from_coefficients_vec((0..10).map(|i| Fr::from(i)).collect());
        assert!(matches!(
            commit_polynomial(&poly, &GLOBAL_PARAMETERS),
            Ok(_)
        ));
    }

    #[test]
    fn generate_proof_and_validate() {
        let mut bytes: [u8; 310] = [0; 310];
        let mut rng = thread_rng();
        bytes.try_fill(&mut rng).unwrap();
        let (eval, poly) = bytes_to_polynomial::<31>(&bytes, *DOMAIN).unwrap();
        let commitment = commit_polynomial(&poly, &GLOBAL_PARAMETERS).unwrap();
        let proofs: Vec<_> = (0..10)
            .map(|i| generate_element_proof(i, &poly, &eval, &GLOBAL_PARAMETERS, *DOMAIN).unwrap())
            .collect();

        eval.evals
            .par_iter()
            .zip(proofs.par_iter())
            .enumerate()
            .for_each(|(i, (element, proof))| {
                for ii in i..10 {
                    if ii == i {
                        // verifying works
                        assert!(verify_element_proof(
                            ii,
                            element,
                            &commitment,
                            proof,
                            *DOMAIN,
                            &GLOBAL_PARAMETERS
                        ));
                    } else {
                        // Verification should fail for other points
                        assert!(!verify_element_proof(
                            ii,
                            element,
                            &commitment,
                            proof,
                            *DOMAIN,
                            &GLOBAL_PARAMETERS
                        ));
                    }
                }
            });
    }
}
