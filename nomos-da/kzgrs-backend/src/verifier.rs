// std

use ark_poly::EvaluationDomain;
// crates
use blst::min_sig::{PublicKey, SecretKey};
use itertools::{izip, Itertools};
use kzgrs::common::field_element_from_bytes_le;
use kzgrs::{
    bytes_to_polynomial, commit_polynomial, verify_element_proof, Commitment,
    PolynomialEvaluationDomain, Proof, BYTES_PER_FIELD_ELEMENT,
};

use crate::common::blob::DaBlob;
// internal
use crate::common::{hash_column_and_commitment, Chunk, Column};
use crate::encoder::DaEncoderParams;
use crate::global::GLOBAL_PARAMETERS;

pub struct DaVerifier {
    // TODO: substitute this for an abstraction to sign things over
    pub sk: SecretKey,
    pub index: usize,
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
        rows_domain: PolynomialEvaluationDomain,
    ) -> bool {
        let column_domain =
            PolynomialEvaluationDomain::new(column.len()).expect("Domain should be able to build");
        // 1. compute commitment for column
        let Ok((_, polynomial)) = bytes_to_polynomial::<BYTES_PER_FIELD_ELEMENT>(
            column.as_bytes().as_slice(),
            column_domain,
        ) else {
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
            rows_domain,
            &GLOBAL_PARAMETERS,
        )
    }

    fn verify_chunk(
        chunk: &Chunk,
        commitment: &Commitment,
        proof: &Proof,
        index: usize,
        domain: PolynomialEvaluationDomain,
    ) -> bool {
        let element = field_element_from_bytes_le(chunk.as_bytes().as_slice());
        verify_element_proof(
            index,
            &element,
            commitment,
            proof,
            domain,
            &GLOBAL_PARAMETERS,
        )
    }

    fn verify_chunks(
        chunks: &[Chunk],
        commitments: &[Commitment],
        proofs: &[Proof],
        index: usize,
        domain: PolynomialEvaluationDomain,
    ) -> bool {
        if ![chunks.len(), commitments.len(), proofs.len()]
            .iter()
            .all_equal()
        {
            return false;
        }
        for (chunk, commitment, proof) in izip!(chunks, commitments, proofs) {
            if !DaVerifier::verify_chunk(chunk, commitment, proof, index, domain) {
                return false;
            }
        }
        true
    }

    pub fn verify(&self, blob: &DaBlob, rows_domain_size: usize) -> bool {
        let rows_domain = PolynomialEvaluationDomain::new(rows_domain_size)
            .expect("Domain should be able to build");
        let is_column_verified = DaVerifier::verify_column(
            &blob.column,
            &blob.column_commitment,
            &blob.aggregated_column_commitment,
            &blob.aggregated_column_proof,
            self.index,
            rows_domain,
        );
        if !is_column_verified {
            return false;
        }

        let are_chunks_verified = DaVerifier::verify_chunks(
            blob.column.as_ref(),
            &blob.rows_commitments,
            &blob.rows_proofs,
            self.index,
            rows_domain,
        );
        if !are_chunks_verified {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod test {
    use crate::common::blob::DaBlob;
    use crate::common::{hash_column_and_commitment, Chunk, Column};
    use crate::encoder::test::{rand_data, ENCODER};
    use crate::encoder::DaEncoderParams;
    use crate::global::GLOBAL_PARAMETERS;
    use crate::verifier::DaVerifier;
    use ark_bls12_381::Fr;
    use ark_poly::{EvaluationDomain, GeneralEvaluationDomain};
    use blst::min_sig::SecretKey;
    use kzgrs::{
        bytes_to_polynomial, commit_polynomial, generate_element_proof,
        global_parameters_from_randomness, Commitment, GlobalParameters,
        PolynomialEvaluationDomain, Proof, BYTES_PER_FIELD_ELEMENT,
    };
    use nomos_core::da::DaEncoder as _;
    use once_cell::sync::Lazy;
    use rand::{thread_rng, RngCore};

    pub struct ColumnVerifyData {
        pub column: Column,
        pub column_commitment: Commitment,
        pub aggregated_commitment: Commitment,
        pub column_proof: Proof,
        pub domain: GeneralEvaluationDomain<Fr>,
    }

    fn prepare_column(
        with_new_global_params: bool,
    ) -> Result<ColumnVerifyData, Box<dyn std::error::Error>> {
        pub static NEW_GLOBAL_PARAMETERS: Lazy<GlobalParameters> = Lazy::new(|| {
            let mut rng = rand::thread_rng();
            global_parameters_from_randomness(&mut rng)
        });

        let mut global_params = &GLOBAL_PARAMETERS;
        if with_new_global_params {
            global_params = &NEW_GLOBAL_PARAMETERS;
        }

        let column: Column = (0..10).map(|i| Chunk(vec![i; 32])).collect();
        let domain = GeneralEvaluationDomain::new(10).unwrap();
        let (_, column_poly) =
            bytes_to_polynomial::<BYTES_PER_FIELD_ELEMENT>(column.as_bytes().as_slice(), domain)?;
        let column_commitment = commit_polynomial(&column_poly, global_params)?;

        let (aggregated_evals, aggregated_poly) = bytes_to_polynomial::<
            { DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE },
        >(
            hash_column_and_commitment::<{ DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE }>(
                &column,
                &column_commitment,
            )
            .as_slice(),
            domain,
        )?;

        let aggregated_commitment = commit_polynomial(&aggregated_poly, global_params)?;

        let column_proof = generate_element_proof(
            0,
            &aggregated_poly,
            &aggregated_evals,
            &global_params,
            domain,
        )?;

        Ok(ColumnVerifyData {
            column,
            column_commitment,
            aggregated_commitment,
            column_proof,
            domain,
        })
    }

    #[test]
    fn test_verify_column() {
        let column_data = prepare_column(false).unwrap();

        assert!(DaVerifier::verify_column(
            &column_data.column,
            &column_data.column_commitment,
            &column_data.aggregated_commitment,
            &column_data.column_proof,
            0,
            column_data.domain
        ));
    }

    #[test]
    fn test_verify_column_error_cases() {
        // Test bytes_to_polynomial() returned error
        let column_data = prepare_column(false).unwrap();

        let column2: Column = (0..10)
            .flat_map(|i| {
                if i % 2 == 0 {
                    vec![Chunk(vec![i; 16])]
                } else {
                    vec![Chunk(vec![i; 32])]
                }
            })
            .collect();

        assert!(bytes_to_polynomial::<BYTES_PER_FIELD_ELEMENT>(
            column2.as_bytes().as_slice(),
            column_data.domain
        )
        .is_err());

        assert!(!DaVerifier::verify_column(
            &column2,
            &column_data.column_commitment,
            &column_data.aggregated_commitment,
            &column_data.column_proof,
            0,
            column_data.domain
        ));

        // Test alter GLOBAL_PARAMETERS so that computed_column_commitment != column_commitment
        let column_data2 = prepare_column(true).unwrap();

        assert!(!DaVerifier::verify_column(
            &column_data2.column,
            &column_data2.column_commitment,
            &column_data2.aggregated_commitment,
            &column_data2.column_proof,
            0,
            column_data2.domain
        ));
    }

    #[test]
    fn test_verify_chunks_error_cases() {
        let encoder = &ENCODER;
        let data = rand_data(32);
        let domain_size = 16usize;
        let rows_domain = PolynomialEvaluationDomain::new(domain_size).unwrap();
        let encoded_data = encoder.encode(&data).unwrap();
        let column = encoded_data.extended_data.columns().next().unwrap();
        let index = 0usize;

        let da_blob = DaBlob {
            column,
            column_idx: index.try_into().unwrap(),
            column_commitment: encoded_data.column_commitments[index],
            aggregated_column_commitment: encoded_data.aggregated_column_commitment,
            aggregated_column_proof: encoded_data.aggregated_column_proofs[index],
            rows_commitments: encoded_data.row_commitments.clone(),
            rows_proofs: encoded_data
                .rows_proofs
                .iter()
                .map(|proofs| proofs.get(index).cloned().unwrap())
                .collect(),
        };
        // Happy case
        let chunks_verified = DaVerifier::verify_chunks(
            da_blob.column.as_ref(),
            &da_blob.rows_commitments,
            &da_blob.rows_proofs,
            index,
            rows_domain,
        );
        assert!(chunks_verified);

        // Modified chunks
        let mut column_w_missing_chunk = da_blob.column.as_ref().to_vec();
        column_w_missing_chunk.pop();
        let chunks_not_verified = !DaVerifier::verify_chunks(
            column_w_missing_chunk.as_ref(),
            &da_blob.rows_commitments,
            &da_blob.rows_proofs,
            index,
            rows_domain,
        );
        assert!(chunks_not_verified);

        // Modified proofs
        let mut modified_proofs = da_blob.rows_proofs.to_vec();
        let proofs_len = modified_proofs.len();
        modified_proofs.swap(0, proofs_len - 1);
        let chunks_not_verified = !DaVerifier::verify_chunks(
            da_blob.column.as_ref(),
            &da_blob.rows_commitments,
            &modified_proofs,
            index,
            rows_domain,
        );
        assert!(chunks_not_verified);

        // Modified commitments
        let mut modified_commitments = da_blob.rows_commitments.to_vec();
        let commitments_len = modified_proofs.len();
        modified_commitments.swap(0, commitments_len - 1);

        let chunks_not_verified = !DaVerifier::verify_chunks(
            da_blob.column.as_ref(),
            &modified_commitments,
            &da_blob.rows_proofs,
            index,
            rows_domain,
        );
        assert!(chunks_not_verified);
    }

    #[test]
    fn test_verify() {
        let encoder = &ENCODER;
        let data = rand_data(32);
        let domain_size = 16usize;
        let mut rng = thread_rng();
        let sks: Vec<SecretKey> = (0..16)
            .map(|_| {
                let mut buff = [0u8; 32];
                rng.fill_bytes(&mut buff);
                SecretKey::key_gen(&buff, &[]).unwrap()
            })
            .collect();
        let verifiers: Vec<DaVerifier> = sks
            .into_iter()
            .enumerate()
            .map(|(index, sk)| DaVerifier { sk, index })
            .collect();
        let encoded_data = encoder.encode(&data).unwrap();
        for (i, column) in encoded_data.extended_data.columns().enumerate() {
            println!("{i}");
            let verifier = &verifiers[i];
            let da_blob = DaBlob {
                column,
                column_idx: i
                    .try_into()
                    .expect("Column index shouldn't overflow the target type"),
                column_commitment: encoded_data.column_commitments[i],
                aggregated_column_commitment: encoded_data.aggregated_column_commitment,
                aggregated_column_proof: encoded_data.aggregated_column_proofs[i],
                rows_commitments: encoded_data.row_commitments.clone(),
                rows_proofs: encoded_data
                    .rows_proofs
                    .iter()
                    .map(|proofs| proofs.get(i).cloned().unwrap())
                    .collect(),
            };
            assert!(verifier.verify(&da_blob, domain_size));
        }
    }
}
