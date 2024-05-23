// std
use std::hash::{Hash, Hasher};

// crates
use bitvec::prelude::*;
use blst::min_sig::{AggregateSignature, PublicKey, Signature};
use blst::BLST_ERROR;
use kzgrs::{Commitment, KzgRsError};
use nomos_core::da::certificate::metadata::Next;
use nomos_core::da::certificate::{self, metadata};

// internal
use crate::common::{attestation::Attestation, build_attestation_message, NOMOS_DA_DST};
use crate::encoder::EncodedData;

#[derive(Debug, Clone, PartialEq)]
pub struct Certificate {
    aggregated_signatures: Signature,
    signers: BitVec<u8>,
    aggregated_column_commitment: Commitment,
    row_commitments: Vec<Commitment>,
    metadata: Metadata,
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
        metadata: Metadata,
    ) -> Result<Self, KzgRsError> {
        if attestations.len() < threshold {
            return Err(KzgRsError::NotEnoughAttestations {
                required: threshold,
                received: attestations.len(),
            });
        }

        if attestations.len() != signers.count_ones() {
            return Err(KzgRsError::AttestationSignersMismatch {
                attestations_count: attestations.len(),
                signers_count: signers.count_ones(),
            });
        }

        let signatures: Vec<Signature> = attestations
            .iter()
            .filter_map(|att| Signature::from_bytes(&att.signature).ok())
            .collect();

        // Certificate will fail to be built if number of valid signatures from the attestations
        // doesn't satisfy the same threshold used for attestations.
        if signatures.len() < threshold {
            return Err(KzgRsError::NotEnoughAttestations {
                required: threshold,
                received: signatures.len(),
            });
        }

        let aggregated_signatures = aggregate_signatures(signatures)?;

        Ok(Self {
            aggregated_signatures,
            signers,
            aggregated_column_commitment: encoded_data.aggregated_column_commitment,
            row_commitments: encoded_data.row_commitments.clone(),
            metadata,
        })
    }
}

fn aggregate_signatures(signatures: Vec<Signature>) -> Result<Signature, BLST_ERROR> {
    let refs: Vec<&Signature> = signatures.iter().collect();
    AggregateSignature::aggregate(&refs, true).map(|agg_sig| agg_sig.to_signature())
}

fn verify_aggregate_signature(
    aggregate_signature: &Signature,
    public_keys: &[&PublicKey],
    messages: &[&[u8]],
) -> bool {
    BLST_ERROR::BLST_SUCCESS
        == aggregate_signature.aggregate_verify(true, messages, NOMOS_DA_DST, public_keys, true)
}

#[derive(Clone, Debug)]
pub struct CertificateVerificationParameters {
    pub nodes_public_keys: Vec<PublicKey>,
}

impl certificate::Certificate for Certificate {
    type Signature = Signature;
    type Id = Vec<u8>;
    type VerificationParameters = CertificateVerificationParameters;

    fn signers(&self) -> Vec<bool> {
        self.signers.iter().map(|b| *b).collect()
    }

    fn signature(&self) -> Self::Signature {
        self.aggregated_signatures
    }

    fn id(&self) -> Self::Id {
        build_attestation_message(&self.aggregated_column_commitment, &self.row_commitments)
    }

    fn verify(&self, params: Self::VerificationParameters) -> bool {
        self.verify(&params.nodes_public_keys)
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct Index([u8; 8]);

#[derive(Default, Debug, Copy, Clone, Eq, PartialEq)]
pub struct Metadata {
    app_id: [u8; 32],
    index: Index,
}

impl Metadata {
    pub fn size(&self) -> usize {
        std::mem::size_of_val(&self.app_id) + std::mem::size_of_val(&self.index)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VidCertificate {
    id: Vec<u8>,
    metadata: Metadata,
}

impl certificate::vid::VidCertificate for VidCertificate {
    type CertificateId = Vec<u8>;

    fn certificate_id(&self) -> Self::CertificateId {
        self.id.clone()
    }

    fn size(&self) -> usize {
        std::mem::size_of_val(&self.id) + self.metadata.size()
    }
}

impl metadata::Metadata for VidCertificate {
    type AppId = [u8; 32];
    type Index = Index;

    fn metadata(&self) -> (Self::AppId, Self::Index) {
        (self.metadata.app_id, self.metadata.index)
    }
}

impl Hash for VidCertificate {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(
            <VidCertificate as certificate::vid::VidCertificate>::certificate_id(self).as_ref(),
        );
    }
}

impl From<Certificate> for VidCertificate {
    fn from(cert: Certificate) -> Self {
        Self {
            id: cert.id(),
            metadata: cert.metadata,
        }
    }
}

impl metadata::Metadata for Certificate {
    type AppId = [u8; 32];
    type Index = Index;

    fn metadata(&self) -> (Self::AppId, Self::Index) {
        (self.metadata.app_id, self.metadata.index)
    }
}

impl From<u64> for Index {
    fn from(value: u64) -> Self {
        Self(value.to_be_bytes())
    }
}

impl Next for Index {
    fn next(self) -> Self {
        let num = u64::from_be_bytes(self.0);
        let incremented_num = num.wrapping_add(1);
        Self(incremented_num.to_be_bytes())
    }
}

impl AsRef<[u8]> for Index {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use bitvec::prelude::*;
    use blst::min_sig::{PublicKey, SecretKey};
    use rand::{rngs::OsRng, thread_rng, Rng, RngCore};

    use crate::{
        common::{blob::DaBlob, NOMOS_DA_DST},
        dispersal::{aggregate_signatures, verify_aggregate_signature, Metadata},
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
        let cert = Certificate::build_certificate(
            &encoded_data,
            &attestations,
            signers,
            16,
            Metadata::default(),
        )
        .unwrap();

        let public_keys: Vec<PublicKey> = sks.iter().map(|sk| sk.sk_to_pk()).collect();
        assert!(cert.verify(&public_keys));
    }
}
