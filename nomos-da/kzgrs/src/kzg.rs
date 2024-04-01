use crate::common::KzgRsError;
use ark_bls12_381::{Bls12_381, Fr};
use ark_poly::univariate::DensePolynomial;
use ark_poly_commit::kzg10::{Commitment, Powers, KZG10};

pub fn commit_polynomial(
    polynomial: &DensePolynomial<Fr>,
    roots_of_unity: &Powers<Bls12_381>,
) -> Result<Commitment<Bls12_381>, KzgRsError> {
    KZG10::commit(roots_of_unity, polynomial, None, None)
        .map_err(KzgRsError::PolyCommitError)
        .map(|(commitment, _)| commitment)
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
