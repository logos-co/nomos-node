use crate::common::attestation::Attestation;
use crate::common::{build_attestation_message, NOMOS_DA_DST};
use crate::encoder::EncodedData;
use bitvec::prelude::*;
use blst::min_sig::{AggregateSignature, PublicKey, Signature};
use blst::BLST_ERROR;
use kzgrs::Commitment;

#[derive(Debug, Clone, PartialEq)]
pub struct Certificate {
    aggregated_signatures: Signature,
    signers: BitVec<u8>,
    aggregated_column_commitment: Commitment,
    row_commitments: Vec<Commitment>,
}

impl Certificate {
    pub fn id(&self) -> Vec<u8> {
        build_attestation_message(&self.aggregated_column_commitment, &self.row_commitments)
    }

    pub fn verify(&self, nodes_public_keys: &[PublicKey]) -> bool {
        let signers_keys: Vec<&PublicKey> = nodes_public_keys
            .iter()
            .enumerate()
            .filter(|(index, _)| self.signers[*index])
            .map(|(_, pk)| pk)
            .collect();

        let message = self.id();
        let messages: Vec<&[u8]> = std::iter::repeat(message.as_slice())
            .take(signers_keys.len())
            .collect();

        verify_aggregate_signature(&self.aggregated_signatures, &signers_keys, &messages)
    }

    pub fn build_certificate(
        encoded_data: &EncodedData,
        attestations: &[Attestation],
        signers: BitVec<u8>,
        threshold: usize,
    ) -> Self {
        assert!(attestations.len() >= threshold, "Not enough attestations");
        assert_eq!(
            attestations.len(),
            signers.count_ones(),
            "Mismatch between attestations and signers count"
        );

        let signatures: Vec<Signature> = attestations
            .iter()
            .map(|att| {
                Signature::from_bytes(&att.signature)
                    .expect("Attestation should have valid bls signature")
            })
            .collect();
        let aggregated_signatures = aggregate_signatures(signatures).expect("");

        Self {
            aggregated_signatures,
            signers,
            aggregated_column_commitment: encoded_data.aggregated_column_commitment,
            row_commitments: encoded_data.row_commitments.clone(),
        }
    }
}

fn aggregate_signatures(signatures: Vec<Signature>) -> Result<Signature, BLST_ERROR> {
    let refs: Vec<&Signature> = signatures.iter().collect();
    let agg_result = AggregateSignature::aggregate(&refs, true);
    agg_result.map(|agg_sig| agg_sig.to_signature())
}

fn verify_aggregate_signature(
    aggregate_signature: &Signature,
    public_keys: &[&PublicKey],
    messages: &[&[u8]],
) -> bool {
    BLST_ERROR::BLST_SUCCESS
        == aggregate_signature.aggregate_verify(true, messages, NOMOS_DA_DST, public_keys, true)
}

#[cfg(test)]
mod tests {
    use bitvec::prelude::*;
    use blst::min_sig::{PublicKey, SecretKey};
    use rand::{rngs::OsRng, thread_rng, Rng, RngCore};

    use crate::{
        common::{blob::DaBlob, NOMOS_DA_DST},
        dispersal::{aggregate_signatures, verify_aggregate_signature},
        encoder::test::{rand_data, ENCODER},
        verifier::DaVerifier,
    };

    use super::Certificate;

    fn generate_keys() -> (PublicKey, SecretKey) {
        let mut rng = OsRng;
        let sk_bytes: [u8; 32] = rng.gen();
        let sk = SecretKey::key_gen(&sk_bytes, &[]).unwrap();
        let pk = sk.sk_to_pk();
        (pk, sk)
    }

    #[test]
    fn test_signature_aggregation_and_verification() {
        let (pk1, sk1) = generate_keys();
        let (pk2, sk2) = generate_keys();
        let (pk3, sk3) = generate_keys();

        let message = b"Test message";
        let sig1 = sk1.sign(message, NOMOS_DA_DST, &[]);
        let sig2 = sk2.sign(message, NOMOS_DA_DST, &[]);
        let sig3 = sk3.sign(message, NOMOS_DA_DST, &[]);

        let aggregated_signature = aggregate_signatures(vec![sig1, sig2, sig3]).unwrap();

        let public_keys = vec![&pk1, &pk2, &pk3];
        let messages = vec![message.as_ref(), message.as_ref(), message.as_ref()];
        let result = verify_aggregate_signature(&aggregated_signature, &public_keys, &messages);

        assert!(result, "Aggregated signature should be valid.");
    }

    #[test]
    fn test_invalid_signature_aggregation() {
        let (pk1, sk1) = generate_keys();
        let (pk2, sk2) = generate_keys();
        let (_, sk3) = generate_keys(); // Wrong secret key for pk2

        let message = b"Test message";
        let sig1 = sk1.sign(message, NOMOS_DA_DST, &[]);
        let sig2 = sk2.sign(message, NOMOS_DA_DST, &[]);
        let sig3 = sk3.sign(message, NOMOS_DA_DST, &[]);

        let aggregated_signature = aggregate_signatures(vec![sig1, sig2, sig3]).unwrap();

        let public_keys = vec![&pk1, &pk2, &pk2]; // Wrong public key for the third signature
        let messages = vec![message.as_ref(), message.as_ref(), message.as_ref()];
        let result = verify_aggregate_signature(&aggregated_signature, &public_keys, &messages);

        assert!(
            !result,
            "Aggregated signature with an invalid signature should not be valid."
        );
    }

    #[test]
    fn test_encoded_data_verification() {
        let encoder = &ENCODER;
        let data = rand_data(8);
        let mut rng = thread_rng();
        let sks: Vec<SecretKey> = (0..16)
            .map(|_| {
                let mut buff = [0u8; 32];
                rng.fill_bytes(&mut buff);
                SecretKey::key_gen(&buff, &[]).unwrap()
            })
            .collect();
        let verifiers: Vec<DaVerifier> = sks
            .clone()
            .into_iter()
            .enumerate()
            .map(|(index, sk)| DaVerifier { sk, index })
            .collect();
        let encoded_data = encoder.encode(&data).unwrap();

        let mut attestations = vec![];
        for (i, column) in encoded_data.extended_data.columns().enumerate() {
            let verifier = &verifiers[i];
            let da_blob = DaBlob {
                column,
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
            attestations.push(verifier.verify(da_blob).unwrap());
        }

        let signers = bitvec![u8, Lsb0; 1; 16];
        let cert = Certificate::build_certificate(&encoded_data, &attestations, signers, 16);

        let public_keys: Vec<PublicKey> = sks.iter().map(|sk| sk.sk_to_pk()).collect();
        assert!(cert.verify(&public_keys));
    }
}
