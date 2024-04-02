use ark_bls12_381::Fr;
use ark_poly::univariate::DensePolynomial;
use ark_poly::{EvaluationDomain, Evaluations, GeneralEvaluationDomain, Polynomial};

pub fn encode(
    polynomial: &DensePolynomial<Fr>,
    evaluations: &Evaluations<Fr>,
    factor: usize,
    domain: &GeneralEvaluationDomain<Fr>,
) -> Evaluations<Fr> {
    assert!(factor > 1);
    Evaluations::from_vec_and_domain(
        (0..evaluations.evals.len() * factor)
            .map(|i| polynomial.evaluate(&domain.element(i)))
            .collect(),
        *domain,
    )
}

pub fn decode(
    original_chunks_len: usize,
    points: &[Fr],
    domain: &GeneralEvaluationDomain<Fr>,
) -> Evaluations<Fr> {
    let evals = Evaluations::<Fr>::from_vec_and_domain(points.to_vec(), *domain);
    let coeffs = evals.interpolate();

    Evaluations::from_vec_and_domain(
        (0..original_chunks_len)
            .map(|i| coeffs.evaluate(&domain.element(i)))
            .collect(),
        *domain,
    )
}
