// std
use std::ops::Div;

// crates
use ark_ff::{BigInteger, PrimeField};
use ark_poly::EvaluationDomain;
use kzgrs::common::bytes_to_polynomial_unchecked;
use kzgrs::fk20::fk20_batch_generate_elements_proofs;
use kzgrs::{
    bytes_to_polynomial, commit_polynomial, encode, Commitment, Evaluations, KzgRsError,
    Polynomial, PolynomialEvaluationDomain, Proof, BYTES_PER_FIELD_ELEMENT,
};

// internal
use crate::common::{hash_column_and_commitment, Chunk, ChunksMatrix, Row};
use crate::global::GLOBAL_PARAMETERS;

#[derive(Copy, Clone)]
pub struct DaEncoderParams {
    column_count: usize,
}

impl DaEncoderParams {
    pub const MAX_BLS12_381_ENCODING_CHUNK_SIZE: usize = 31;

    pub const fn default_with(column_count: usize) -> Self {
        Self { column_count }
    }
}

pub struct EncodedData {
    pub data: Vec<u8>,
    pub chunked_data: ChunksMatrix,
    pub extended_data: ChunksMatrix,
    pub row_commitments: Vec<Commitment>,
    pub rows_proofs: Vec<Vec<Proof>>,
    pub column_commitments: Vec<Commitment>,
    pub aggregated_column_commitment: Commitment,
    pub aggregated_column_proofs: Vec<Proof>,
}

pub struct DaEncoder {
    params: DaEncoderParams,
}

impl DaEncoder {
    pub const fn new(settings: DaEncoderParams) -> Self {
        Self { params: settings }
    }

    fn chunkify(&self, data: &[u8]) -> ChunksMatrix {
        let chunk_size =
            // column count is divided by two, as later on rows are encoded to twice the size
            self.params.column_count.div(2) * DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE;
        data.chunks(chunk_size)
            .map(|d| {
                d.chunks(DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE)
                    .map(|chunk| {
                        let mut buff = [0u8; BYTES_PER_FIELD_ELEMENT];
                        buff[..DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE]
                            .copy_from_slice(chunk);
                        Chunk::from(buff.as_slice())
                    })
                    .collect()
            })
            .collect()
    }

    #[allow(clippy::type_complexity)]
    fn compute_kzg_row_commitments(
        matrix: &ChunksMatrix,
        polynomial_evaluation_domain: PolynomialEvaluationDomain,
    ) -> Result<Vec<((Evaluations, Polynomial), Commitment)>, KzgRsError> {
        matrix
            .rows()
            .map(|r| {
                // Using the unchecked version here. Because during the process of chunkifiying
                // we already make sure to have the chunks of proper elements.
                // Also, after rs encoding, we are sure all `Fr` elements already fits within modulus.
                let (evals, poly) = bytes_to_polynomial_unchecked::<BYTES_PER_FIELD_ELEMENT>(
                    r.as_bytes().as_ref(),
                    polynomial_evaluation_domain,
                );
                commit_polynomial(&poly, &GLOBAL_PARAMETERS)
                    .map(|commitment| ((evals, poly), commitment))
            })
            .collect()
    }

    fn rs_encode_row(
        row: &Polynomial,
        polynomial_evaluation_domain: PolynomialEvaluationDomain,
    ) -> Evaluations {
        encode(row, polynomial_evaluation_domain)
    }

    fn rs_encode_rows(
        rows: &[Polynomial],
        polynomial_evaluation_domain: PolynomialEvaluationDomain,
    ) -> Vec<Evaluations> {
        rows.iter()
            .map(|poly| Self::rs_encode_row(poly, polynomial_evaluation_domain))
            .collect()
    }

    fn compute_rows_proofs(polynomials: &[Polynomial]) -> Result<Vec<Vec<Proof>>, KzgRsError> {
        Ok(polynomials
            .iter()
            .map(|poly| fk20_batch_generate_elements_proofs(poly, &GLOBAL_PARAMETERS))
            .collect())
    }

    #[allow(clippy::type_complexity)]
    fn compute_kzg_column_commitments(
        matrix: &ChunksMatrix,
        polynomial_evaluation_domain: PolynomialEvaluationDomain,
    ) -> Result<Vec<((Evaluations, Polynomial), Commitment)>, KzgRsError> {
        Self::compute_kzg_row_commitments(&matrix.transposed(), polynomial_evaluation_domain)
    }

    fn compute_aggregated_column_commitment(
        matrix: &ChunksMatrix,
        commitments: &[Commitment],
        polynomial_evaluation_domain: PolynomialEvaluationDomain,
    ) -> Result<((Evaluations, Polynomial), Commitment), KzgRsError> {
        let hashes: Vec<u8> =
            matrix
                .columns()
                .zip(commitments)
                .flat_map(|(column, commitment)| {
                    hash_column_and_commitment::<
                        { DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE },
                    >(&column, commitment)
                })
                .collect();
        let (evals, poly) = bytes_to_polynomial::<
            { DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE },
        >(hashes.as_ref(), polynomial_evaluation_domain)?;
        let commitment = commit_polynomial(&poly, &GLOBAL_PARAMETERS)?;
        Ok(((evals, poly), commitment))
    }

    fn compute_aggregated_column_proofs(polynomial: &Polynomial) -> Result<Vec<Proof>, KzgRsError> {
        Ok(fk20_batch_generate_elements_proofs(
            polynomial,
            &GLOBAL_PARAMETERS,
        ))
    }

    fn evals_to_chunk_matrix(evals: &[Evaluations]) -> ChunksMatrix {
        ChunksMatrix(
            evals
                .iter()
                .map(|eval| {
                    Row(eval
                        .evals
                        .iter()
                        .map(|point| Chunk(point.into_bigint().to_bytes_le()))
                        .collect())
                })
                .collect(),
        )
    }

    pub fn encode(&self, data: &[u8]) -> Result<EncodedData, kzgrs::KzgRsError> {
        let chunked_data = self.chunkify(data);
        let row_domain = PolynomialEvaluationDomain::new(self.params.column_count)
            .expect("Domain should be able to build");
        let column_domain = PolynomialEvaluationDomain::new(chunked_data.len())
            .expect("Domain should be able to build");
        let (row_polynomials, row_commitments): (Vec<_>, Vec<_>) =
            Self::compute_kzg_row_commitments(&chunked_data, row_domain)?
                .into_iter()
                .unzip();
        let (_, row_polynomials): (Vec<_>, Vec<_>) = row_polynomials.into_iter().unzip();
        let encoded_evaluations = Self::rs_encode_rows(&row_polynomials, row_domain);
        let extended_data = Self::evals_to_chunk_matrix(&encoded_evaluations);
        let rows_proofs = Self::compute_rows_proofs(&row_polynomials)?;
        let (_column_polynomials, column_commitments): (Vec<_>, Vec<_>) =
            Self::compute_kzg_column_commitments(&extended_data, column_domain)?
                .into_iter()
                .unzip();
        let ((_aggregated_evals, aggregated_polynomial), aggregated_column_commitment) =
            Self::compute_aggregated_column_commitment(
                &extended_data,
                &column_commitments,
                row_domain,
            )?;
        let aggregated_column_proofs =
            Self::compute_aggregated_column_proofs(&aggregated_polynomial)?;
        Ok(EncodedData {
            data: data.to_vec(),
            chunked_data,
            extended_data,
            row_commitments,
            rows_proofs,
            column_commitments,
            aggregated_column_commitment,
            aggregated_column_proofs,
        })
    }
}

#[cfg(test)]
pub mod test {
    use crate::encoder::{DaEncoder, DaEncoderParams};
    use crate::global::GLOBAL_PARAMETERS;
    use ark_ff::PrimeField;
    use ark_poly::{EvaluationDomain, GeneralEvaluationDomain};
    use itertools::izip;
    use kzgrs::common::bytes_to_polynomial_unchecked;
    use kzgrs::{
        decode, verify_element_proof, FieldElement, PolynomialEvaluationDomain,
        BYTES_PER_FIELD_ELEMENT,
    };
    use rand::RngCore;
    use std::ops::Div;

    pub const DOMAIN_SIZE: usize = 16;
    pub const PARAMS: DaEncoderParams = DaEncoderParams::default_with(DOMAIN_SIZE);
    pub const ENCODER: DaEncoder = DaEncoder::new(PARAMS);

    pub fn rand_data(elements_count: usize) -> Vec<u8> {
        let mut buff = vec![0; elements_count * DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE];
        rand::thread_rng().fill_bytes(&mut buff);
        buff
    }

    #[test]
    fn test_chunkify() {
        let params = DaEncoderParams::default_with(2);
        let elements = 10usize;
        let data = rand_data(elements);
        let encoder = DaEncoder::new(params);
        let matrix = encoder.chunkify(&data);
        assert_eq!(matrix.len(), elements.div(params.column_count.div(2)));
        for row in matrix.rows() {
            assert_eq!(row.len(), params.column_count.div(2));
            assert_eq!(row.0[0].len(), BYTES_PER_FIELD_ELEMENT);
        }
    }

    #[test]
    fn test_compute_row_kzg_commitments() {
        let data = rand_data(32);
        let domain = GeneralEvaluationDomain::new(DOMAIN_SIZE).unwrap();
        let matrix = ENCODER.chunkify(data.as_ref());
        let commitments_data = DaEncoder::compute_kzg_row_commitments(&matrix, domain).unwrap();
        assert_eq!(commitments_data.len(), matrix.len());
    }

    #[test]
    fn test_evals_to_chunk_matrix() {
        let data = rand_data(32);
        let matrix = ENCODER.chunkify(data.as_ref());
        let domain = PolynomialEvaluationDomain::new(DOMAIN_SIZE).unwrap();
        let (poly_data, _): (Vec<_>, Vec<_>) =
            DaEncoder::compute_kzg_row_commitments(&matrix, domain)
                .unwrap()
                .into_iter()
                .unzip();
        let (_, poly_data): (Vec<_>, Vec<_>) = poly_data.into_iter().unzip();
        let extended_rows = DaEncoder::rs_encode_rows(&poly_data, domain);
        let extended_matrix = DaEncoder::evals_to_chunk_matrix(&extended_rows);
        for (r1, r2) in izip!(matrix.iter(), extended_matrix.iter()) {
            for (c1, c2) in izip!(r1.iter(), r2.iter()) {
                assert_eq!(c1, c2);
            }
        }
    }

    #[test]
    fn test_rs_encode_rows() {
        let data = rand_data(32);
        let domain = GeneralEvaluationDomain::new(DOMAIN_SIZE).unwrap();
        let matrix = ENCODER.chunkify(data.as_ref());
        let (poly_data, _): (Vec<_>, Vec<_>) =
            DaEncoder::compute_kzg_row_commitments(&matrix, domain)
                .unwrap()
                .into_iter()
                .unzip();
        let (evals, polynomials): (Vec<_>, Vec<_>) = poly_data.into_iter().unzip();
        let extended_rows = DaEncoder::rs_encode_rows(&polynomials, domain);
        // check encoding went well, original evaluation points vs extended ones
        for (e1, e2) in izip!(evals.iter(), extended_rows.iter()) {
            for (c1, c2) in izip!(&e1.evals, &e2.evals) {
                assert_eq!(c1, c2);
            }
        }
        let extended_matrix = DaEncoder::evals_to_chunk_matrix(&extended_rows);
        for (r1, r2, evals) in izip!(matrix.iter(), extended_matrix.iter(), extended_rows) {
            assert_eq!(r1.len(), r2.len().div(2));
            for (c1, c2) in izip!(r1.iter(), r2.iter()) {
                assert_eq!(c1, c2);
            }
            let points: Vec<_> = evals.evals.iter().cloned().map(Some).collect();
            let poly_2 = decode(r1.len(), &points, domain);
            let (poly_1, _) = bytes_to_polynomial_unchecked::<BYTES_PER_FIELD_ELEMENT>(
                r1.as_bytes().as_ref(),
                domain,
            );
            assert_eq!(poly_1, poly_2);
        }
    }

    #[test]
    fn test_compute_row_proofs() {
        let data = rand_data(32);
        let domain = GeneralEvaluationDomain::new(DOMAIN_SIZE).unwrap();
        let matrix = ENCODER.chunkify(data.as_ref());
        let (poly_data, commitments): (Vec<_>, Vec<_>) =
            DaEncoder::compute_kzg_row_commitments(&matrix, domain)
                .unwrap()
                .into_iter()
                .unzip();
        let (_evals, polynomials): (Vec<_>, Vec<_>) = poly_data.into_iter().unzip();
        let extended_evaluations = DaEncoder::rs_encode_rows(&polynomials, domain);
        let extended_matrix = DaEncoder::evals_to_chunk_matrix(&extended_evaluations);
        let proofs = DaEncoder::compute_rows_proofs(&polynomials).unwrap();

        let checks = izip!(matrix.iter(), &commitments, &proofs);
        for (row, commitment, proofs) in checks {
            assert_eq!(proofs.len(), row.len() * 2);
            for (i, chunk) in row.iter().enumerate() {
                let element = FieldElement::from_le_bytes_mod_order(chunk.as_bytes().as_ref());
                assert!(verify_element_proof(
                    i,
                    &element,
                    &commitment,
                    &proofs[i],
                    domain,
                    &GLOBAL_PARAMETERS
                ));
            }
        }
        let checks = izip!(extended_matrix.iter(), &commitments, &proofs);
        for (row, commitment, proofs) in checks {
            assert_eq!(proofs.len(), row.len());
            for (i, chunk) in row.iter().enumerate() {
                let element = FieldElement::from_le_bytes_mod_order(chunk.as_bytes().as_ref());
                assert!(verify_element_proof(
                    i,
                    &element,
                    &commitment,
                    &proofs[i],
                    domain,
                    &GLOBAL_PARAMETERS
                ));
            }
        }
    }

    #[test]
    fn test_compute_column_kzg_commitments() {
        let data = rand_data(32);
        let domain = GeneralEvaluationDomain::new(DOMAIN_SIZE).unwrap();
        let matrix = ENCODER.chunkify(data.as_ref());
        let commitments_data = DaEncoder::compute_kzg_column_commitments(&matrix, domain).unwrap();
        assert_eq!(commitments_data.len(), matrix.columns().count());
    }

    #[test]
    fn test_compute_aggregated_column_kzg_commitment() {
        let data = rand_data(32);
        let matrix = ENCODER.chunkify(data.as_ref());
        let domain = GeneralEvaluationDomain::new(DOMAIN_SIZE).unwrap();
        let (_, commitments): (Vec<_>, Vec<_>) =
            DaEncoder::compute_kzg_column_commitments(&matrix, domain)
                .unwrap()
                .into_iter()
                .unzip();
        let _ =
            DaEncoder::compute_aggregated_column_commitment(&matrix, &commitments, domain).unwrap();
    }

    #[test]
    fn test_compute_aggregated_column_kzg_proofs() {
        let data = rand_data(32);
        let matrix = ENCODER.chunkify(data.as_ref());
        let domain = GeneralEvaluationDomain::new(DOMAIN_SIZE).unwrap();
        let (_poly_data, commitments): (Vec<_>, Vec<_>) =
            DaEncoder::compute_kzg_column_commitments(&matrix, domain)
                .unwrap()
                .into_iter()
                .unzip();
        let ((_evals, polynomial), _aggregated_commitment) =
            DaEncoder::compute_aggregated_column_commitment(&matrix, &commitments, domain).unwrap();
        DaEncoder::compute_aggregated_column_proofs(&polynomial).unwrap();
    }

    #[test]
    fn test_full_encode_flow() {
        let data = rand_data(32);
        let domain = GeneralEvaluationDomain::new(DOMAIN_SIZE).unwrap();
        let encoding_data = ENCODER.encode(&data).unwrap();
        assert_eq!(encoding_data.data, data);
        assert_eq!(encoding_data.row_commitments.len(), 4);
        assert_eq!(encoding_data.column_commitments.len(), 16);
        assert_eq!(encoding_data.rows_proofs.len(), 4);
        assert_eq!(encoding_data.rows_proofs[0].len(), 16);
        assert_eq!(encoding_data.aggregated_column_proofs.len(), 16);
        for (row, proofs, commitment) in izip!(
            encoding_data.extended_data.rows(),
            encoding_data.rows_proofs,
            encoding_data.row_commitments
        ) {
            for (chunk_idx, chunk) in row.iter().enumerate() {
                let element = FieldElement::from_le_bytes_mod_order(chunk.as_bytes().as_ref());
                assert!(verify_element_proof(
                    chunk_idx,
                    &element,
                    &commitment,
                    &proofs[chunk_idx],
                    domain,
                    &GLOBAL_PARAMETERS
                ));
            }
        }
    }
}
