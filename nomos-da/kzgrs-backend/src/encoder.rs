use crate::common::{Chunk, ChunksMatrix};
use crate::global::{DOMAIN, GLOBAL_PARAMETERS};
use kzgrs::{
    bytes_to_polynomial, commit_polynomial, encode, generate_element_proof, Commitment,
    Evaluations, Polynomial, PolynomialEvaluationDomain, Proof, BYTES_PER_FIELD_ELEMENT,
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
        matrix: ChunksMatrix,
    ) -> Vec<((Evaluations, Polynomial), Commitment)> {
        matrix
            .rows()
            .map(|r| {
                let (evals, poly) = bytes_to_polynomial::<
                    { DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE },
                >(r.as_bytes().as_ref(), *DOMAIN)
                .unwrap();

                let commitment = commit_polynomial(&poly, &GLOBAL_PARAMETERS).unwrap();
                ((evals, poly), commitment)
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
    ) -> Vec<Vec<Proof>> {
        polynomials
            .iter()
            .zip(commitments.iter())
            .map(|(poly, commitment)| {
                (0..size)
                    .map(|i| generate_element_proof(i, poly, &GLOBAL_PARAMETERS, &DOMAIN).unwrap())
                    .collect()
            })
            .collect()
    }
}
