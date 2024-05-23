use crate::common::{build_attestation_message, Attestation};
use crate::encoder::EncodedData;
use bitvec::prelude::*;
use blst::min_sig::{AggregateSignature, PublicKey, Signature};
use blst::BLST_ERROR;
use kzgrs::Commitment;

const DST_BLS12381: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

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

        let signatures: Vec<Signature> = attestations.iter().map(|att| att.signature).collect();
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
        == aggregate_signature.aggregate_verify(true, messages, DST_BLS12381, public_keys, true)
}

#[cfg(test)]
mod tests {
    use blst::min_sig::{PublicKey, SecretKey};
    use rand::{rngs::OsRng, Rng};

    use crate::dispersal::{aggregate_signatures, verify_aggregate_signature, DST_BLS12381};

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
        let sig1 = sk1.sign(message, DST_BLS12381, &[]);
        let sig2 = sk2.sign(message, DST_BLS12381, &[]);
        let sig3 = sk3.sign(message, DST_BLS12381, &[]);

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
        let sig1 = sk1.sign(message, DST_BLS12381, &[]);
        let sig2 = sk2.sign(message, DST_BLS12381, &[]);
        let sig3 = sk3.sign(message, DST_BLS12381, &[]);

        let aggregated_signature = aggregate_signatures(vec![sig1, sig2, sig3]).unwrap();

        let public_keys = vec![&pk1, &pk2, &pk2]; // Wrong public key for the third signature
        let messages = vec![message.as_ref(), message.as_ref(), message.as_ref()];
        let result = verify_aggregate_signature(&aggregated_signature, &public_keys, &messages);

        assert!(
            !result,
            "Aggregated signature with an invalid signature should not be valid."
        );
    }
}
