// std
use std::ops::Div;
// crates
use ark_ff::{BigInteger, PrimeField};
// internal
use crate::common::{hash_column_and_commitment, Chunk, ChunksMatrix, Row};
use crate::global::{DOMAIN, GLOBAL_PARAMETERS};
use kzgrs::common::bytes_to_polynomial_unchecked;
use kzgrs::{
    bytes_to_polynomial, commit_polynomial, encode, generate_element_proof, Commitment,
    Evaluations, KzgRsError, Polynomial, Proof, BYTES_PER_FIELD_ELEMENT,
};

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
    ) -> Result<Vec<((Evaluations, Polynomial), Commitment)>, KzgRsError> {
        matrix
            .rows()
            .map(|r| {
                // Using the unchecked version here. Because during the process of chunkifiying
                // we already make sure to have the chunks of proper elements.
                // Also, after rs encoding, we are sure all `Fr` elements already fits within modulus.
                let (evals, poly) = bytes_to_polynomial_unchecked::<BYTES_PER_FIELD_ELEMENT>(
                    r.as_bytes().as_ref(),
                    *DOMAIN,
                );
                commit_polynomial(&poly, &GLOBAL_PARAMETERS)
                    .map(|commitment| ((evals, poly), commitment))
            })
            .collect()
    }

    fn rs_encode_row(evaluations: &Evaluations, row: &Polynomial) -> Evaluations {
        encode(row, evaluations, 2, *DOMAIN)
    }

    fn rs_encode_rows(rows: &[(Evaluations, Polynomial)]) -> Vec<Evaluations> {
        rows.iter()
            .map(|(eval, poly)| Self::rs_encode_row(eval, poly))
            .collect()
    }

    fn compute_rows_proofs(
        polynomials: &[Polynomial],
        evals: &[Evaluations],
        proof_count: usize,
    ) -> Result<Vec<Vec<Proof>>, KzgRsError> {
        polynomials
            .iter()
            .zip(evals)
            .map(|(poly, eval)| {
                (0..proof_count)
                    .map(|i| generate_element_proof(i, poly, eval, &GLOBAL_PARAMETERS, *DOMAIN))
                    .collect()
            })
            .collect()
    }

    #[allow(clippy::type_complexity)]
    fn compute_kzg_column_commitments(
        matrix: &ChunksMatrix,
    ) -> Result<Vec<((Evaluations, Polynomial), Commitment)>, KzgRsError> {
        Self::compute_kzg_row_commitments(&matrix.transposed())
    }

    fn compute_aggregated_column_commitment(
        matrix: &ChunksMatrix,
        commitments: &[Commitment],
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
        >(hashes.as_ref(), *DOMAIN)?;
        let commitment = commit_polynomial(&poly, &GLOBAL_PARAMETERS)?;
        Ok(((evals, poly), commitment))
    }

    fn compute_aggregated_column_proofs(
        polynomial: &Polynomial,
        evals: &Evaluations,
        proof_count: usize,
    ) -> Result<Vec<Proof>, KzgRsError> {
        (0..proof_count)
            .map(|i| generate_element_proof(i, polynomial, evals, &GLOBAL_PARAMETERS, *DOMAIN))
            .collect()
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
        let (row_polynomials, row_commitments): (Vec<_>, Vec<_>) =
            Self::compute_kzg_row_commitments(&chunked_data)?
                .into_iter()
                .unzip();
        let encoded_evaluations = Self::rs_encode_rows(&row_polynomials);
        let extended_data = Self::evals_to_chunk_matrix(&encoded_evaluations);
        let row_polynomials: Vec<_> = row_polynomials.into_iter().map(|(_, p)| p).collect();
        let rows_proofs = Self::compute_rows_proofs(
            &row_polynomials,
            &encoded_evaluations,
            self.params.column_count,
        )?;
        let (_column_polynomials, column_commitments): (Vec<_>, Vec<_>) =
            Self::compute_kzg_column_commitments(&extended_data)?
                .into_iter()
                .unzip();
        let ((aggregated_evals, aggregated_polynomial), aggregated_column_commitment) =
            Self::compute_aggregated_column_commitment(&extended_data, &column_commitments)?;
        let aggregated_column_proofs = Self::compute_aggregated_column_proofs(
            &aggregated_polynomial,
            &aggregated_evals,
            column_commitments.len(),
        )?;
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
    use crate::global::{DOMAIN, GLOBAL_PARAMETERS};
    use ark_ff::PrimeField;
    use itertools::izip;
    use kzgrs::common::bytes_to_polynomial_unchecked;
    use kzgrs::{decode, verify_element_proof, FieldElement, BYTES_PER_FIELD_ELEMENT};
    use rand::RngCore;
    use std::ops::Div;

    pub const PARAMS: DaEncoderParams = DaEncoderParams::default_with(16);
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
        let matrix = ENCODER.chunkify(data.as_ref());
        let commitments_data = DaEncoder::compute_kzg_row_commitments(&matrix).unwrap();
        assert_eq!(commitments_data.len(), matrix.len());
    }

    #[test]
    fn test_evals_to_chunk_matrix() {
        let data = rand_data(32);
        let matrix = ENCODER.chunkify(data.as_ref());
        let (poly_data, _): (Vec<_>, Vec<_>) = DaEncoder::compute_kzg_row_commitments(&matrix)
            .unwrap()
            .into_iter()
            .unzip();
        let extended_rows = DaEncoder::rs_encode_rows(&poly_data);
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
        let matrix = ENCODER.chunkify(data.as_ref());
        let (poly_data, _): (Vec<_>, Vec<_>) = DaEncoder::compute_kzg_row_commitments(&matrix)
            .unwrap()
            .into_iter()
            .unzip();
        let extended_rows = DaEncoder::rs_encode_rows(&poly_data);
        let (evals, _): (Vec<_>, Vec<_>) = poly_data.into_iter().unzip();
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
            let poly_2 = decode(r1.len(), &points, *DOMAIN);
            let (poly_1, _) = bytes_to_polynomial_unchecked::<BYTES_PER_FIELD_ELEMENT>(
                r1.as_bytes().as_ref(),
                *DOMAIN,
            );
            assert_eq!(poly_1, poly_2);
        }
    }

    #[test]
    fn test_compute_row_proofs() {
        let data = rand_data(32);
        let matrix = ENCODER.chunkify(data.as_ref());
        let (poly_data, commitments): (Vec<_>, Vec<_>) =
            DaEncoder::compute_kzg_row_commitments(&matrix)
                .unwrap()
                .into_iter()
                .unzip();
        let extended_evaluations = DaEncoder::rs_encode_rows(&poly_data);
        let (evals, polynomials): (Vec<_>, Vec<_>) = poly_data.into_iter().unzip();
        let extended_matrix = DaEncoder::evals_to_chunk_matrix(&extended_evaluations);
        let original_proofs =
            DaEncoder::compute_rows_proofs(&polynomials, &evals, PARAMS.column_count.div(2))
                .unwrap();
        let extended_proofs = DaEncoder::compute_rows_proofs(
            &polynomials,
            &extended_evaluations,
            PARAMS.column_count,
        )
        .unwrap();

        let checks = izip!(matrix.iter(), &commitments, &original_proofs);
        for (row, commitment, proofs) in checks {
            assert_eq!(proofs.len(), row.len());
            for (i, chunk) in row.iter().enumerate() {
                let element = FieldElement::from_le_bytes_mod_order(chunk.as_bytes().as_ref());
                assert!(verify_element_proof(
                    i,
                    &element,
                    &commitment,
                    &proofs[i],
                    *DOMAIN,
                    &GLOBAL_PARAMETERS
                ));
            }
        }
        let checks = izip!(extended_matrix.iter(), &commitments, &extended_proofs);
        for (row, commitment, proofs) in checks {
            assert_eq!(proofs.len(), row.len());
            for (i, chunk) in row.iter().enumerate() {
                let element = FieldElement::from_le_bytes_mod_order(chunk.as_bytes().as_ref());
                assert!(verify_element_proof(
                    i,
                    &element,
                    &commitment,
                    &proofs[i],
                    *DOMAIN,
                    &GLOBAL_PARAMETERS
                ));
            }
        }
    }

    #[test]
    fn test_compute_column_kzg_commitments() {
        let data = rand_data(32);
        let matrix = ENCODER.chunkify(data.as_ref());
        let commitments_data = DaEncoder::compute_kzg_column_commitments(&matrix).unwrap();
        assert_eq!(commitments_data.len(), matrix.columns().count());
    }

    #[test]
    fn test_compute_aggregated_column_kzg_commitment() {
        let data = rand_data(32);
        let matrix = ENCODER.chunkify(data.as_ref());
        let (_, commitments): (Vec<_>, Vec<_>) = DaEncoder::compute_kzg_column_commitments(&matrix)
            .unwrap()
            .into_iter()
            .unzip();
        let _ = DaEncoder::compute_aggregated_column_commitment(&matrix, &commitments).unwrap();
    }

    #[test]
    fn test_compute_aggregated_column_kzg_proofs() {
        let data = rand_data(32);
        let matrix = ENCODER.chunkify(data.as_ref());
        let (_poly_data, commitments): (Vec<_>, Vec<_>) =
            DaEncoder::compute_kzg_column_commitments(&matrix)
                .unwrap()
                .into_iter()
                .unzip();
        let ((evals, polynomial), _aggregated_commitment) =
            DaEncoder::compute_aggregated_column_commitment(&matrix, &commitments).unwrap();
        DaEncoder::compute_aggregated_column_proofs(&polynomial, &evals, commitments.len())
            .unwrap();
    }
}
