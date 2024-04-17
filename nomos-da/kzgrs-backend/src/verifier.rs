// std

// crates
use blst::min_sig::{PublicKey, SecretKey, Signature};
use itertools::Itertools;
use num_bigint::BigUint;
use sha3::{Digest, Sha3_256};

// internal
use crate::common::{build_attestation_message, hash_column_and_commitment, Column};
use crate::encoder::DaEncoderParams;
use crate::global::{DOMAIN, GLOBAL_PARAMETERS};
use kzgrs::common::field_element_from_bytes_le;
use kzgrs::{
    bytes_to_polynomial, commit_polynomial, verify_element_proof, Commitment, FieldElement, Proof,
    BYTES_PER_FIELD_ELEMENT,
};

pub struct DaBlob {
    column: Column,
    column_commitment: Commitment,
    aggregated_column_commitment: Commitment,
    aggregated_column_proof: Proof,
    rows_commitments: Vec<Commitment>,
    rows_proofs: Vec<Proof>,
}

impl DaBlob {
    pub fn id(&self) -> Vec<u8> {
        build_attestation_message(&self.aggregated_column_commitment, &self.rows_commitments)
    }

    pub fn column_id(&self) -> Vec<u8> {
        let mut hasher = Sha3_256::new();
        hasher.update(self.column.as_bytes());
        hasher.finalize().as_slice().to_vec()
    }
}

pub struct DaVerifier {
    // TODO: substitute this for an abstraction to sign things over
    sk: SecretKey,
    index: usize,
}

impl DaVerifier {
    pub fn new(sk: SecretKey, nodes_public_keys: &[PublicKey]) -> Self {
        // TODO: `is_sorted` is experimental, and by contract `nodes_public_keys` should be shorted
        // but not sure how we could enforce it here without re-sorting anyway.
        // assert!(nodes_public_keys.is_sorted());
        let self_pk = sk.sk_to_pk();
        let (index, _) = nodes_public_keys
            .iter()
            .find_position(|&pk| pk == &self_pk)
            .expect("Self pk should be registered");
        Self { sk, index }
    }

    fn verify_column(
        column: &Column,
        column_commitment: &Commitment,
        aggregated_column_commitment: &Commitment,
        aggregated_column_proof: &Proof,
        index: usize,
    ) -> bool {
        // 1. compute commitment for column
        let Ok((_, polynomial)) =
            bytes_to_polynomial::<BYTES_PER_FIELD_ELEMENT>(column.as_bytes().as_slice(), *DOMAIN)
        else {
            return false;
        };
        let Ok(computed_column_commitment) = commit_polynomial(&polynomial, &GLOBAL_PARAMETERS)
        else {
            return false;
        };
        // 2. if computed column commitment != column commitment, fail
        if &computed_column_commitment != column_commitment {
            return false;
        }
        // 3. compute column hash
        let column_hash = hash_column_and_commitment::<
            { DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE },
        >(column, column_commitment);
        // 4. check proof with commitment and proof over the aggregated column commitment
        let element = field_element_from_bytes_le(column_hash.as_slice());
        verify_element_proof(
            index,
            &element,
            aggregated_column_commitment,
            aggregated_column_proof,
            *DOMAIN,
            &GLOBAL_PARAMETERS,
        )
    }
}
