use ark_poly::EvaluationDomain;
use kzgrs::{global_parameters_from_randomness, GlobalParameters, PolynomialEvaluationDomain};
use once_cell::sync::Lazy;

pub static GLOBAL_PARAMETERS: Lazy<GlobalParameters> = Lazy::new(|| {
    let mut rng = rand::thread_rng();
    global_parameters_from_randomness(&mut rng)
});

pub static DOMAIN: Lazy<PolynomialEvaluationDomain> =
    Lazy::new(|| PolynomialEvaluationDomain::new(8192).unwrap());
