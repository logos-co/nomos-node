use crate::common::{hash_column_and_commitment, Chunk, ChunksMatrix};
use crate::global::{DOMAIN, GLOBAL_PARAMETERS};
use ark_poly::univariate::DensePolynomial;
use kzgrs::{
    bytes_to_polynomial, commit_polynomial, encode, generate_element_proof, Commitment,
    Evaluations, KzgRsError, Polynomial, PolynomialEvaluationDomain, Proof,
    BYTES_PER_FIELD_ELEMENT,
};
pub struct DaEncoderParams {
    column_count: usize,
}

impl DaEncoderParams {
    const MAX_BLS12_381_ENCODING_CHUNK_SIZE: usize = 31;

    const fn default_with(column_count: usize) -> Self {
        Self { column_count }
    }
}

pub struct EncodedData {
    data: Vec<u8>,
    chunked_data: ChunksMatrix,
    extended_data: ChunksMatrix,
    row_commitments: Vec<Commitment>,
    row_proofs: Vec<Vec<Proof>>,
    column_commitments: Vec<Commitment>,
    aggregated_column_commitment: Commitment,
    aggregated_column_proofs: Vec<Proof>,
}

struct DaEncoder {
    params: DaEncoderParams,
}

impl DaEncoder {
    pub const fn new(settings: DaEncoderParams) -> Self {
        Self { params: settings }
    }

    fn chunkify(&self, data: &[u8]) -> ChunksMatrix {
        let size = self.params.column_count * DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE;
        data.windows(size)
            .map(|d| {
                d.windows(DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE)
                    .map(Chunk::from)
                    .collect()
            })
            .collect()
    }

    fn compute_kzg_row_commitments(
        matrix: &ChunksMatrix,
    ) -> Result<Vec<((Evaluations, Polynomial), Commitment)>, KzgRsError> {
        matrix
            .rows()
            .map(|r| {
                bytes_to_polynomial::<{ DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE }>(
                    r.as_bytes().as_ref(),
                    *DOMAIN,
                )
                .and_then(|(evals, poly)| {
                    commit_polynomial(&poly, &GLOBAL_PARAMETERS)
                        .map(|commitment| ((evals, poly), commitment))
                })
            })
            .collect()
    }

    fn rs_encode_row(evaluations: &Evaluations, row: &Polynomial) -> Evaluations {
        encode(row, evaluations, 2, &DOMAIN)
    }

    fn rs_encode_rows(rows: &[(Evaluations, Polynomial)]) -> Vec<Evaluations> {
        rows.iter()
            .map(|(eval, poly)| Self::rs_encode_row(eval, poly))
            .collect()
    }

    fn compute_rows_proofs(
        polynomials: &[Polynomial],
        commitments: &[Commitment],
        size: usize,
    ) -> Result<Vec<Vec<Proof>>, KzgRsError> {
        polynomials
            .iter()
            .zip(commitments.iter())
            .map(|(poly, commitment)| {
                (0..size)
                    .map(|i| generate_element_proof(i, poly, &GLOBAL_PARAMETERS, *DOMAIN))
                    .collect()
            })
            .collect()
    }

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
        column_commitments: &[Commitment],
    ) -> Result<Vec<Proof>, KzgRsError> {
        column_commitments
            .iter()
            .enumerate()
            .map(|(i, commitment)| {
                generate_element_proof(i, polynomial, &GLOBAL_PARAMETERS, *DOMAIN)
            })
            .collect()
    }

    pub fn encode(&self, data: &[u8]) -> Result<EncodedData, kzgrs::KzgRsError> {
        todo!()
    }
}
