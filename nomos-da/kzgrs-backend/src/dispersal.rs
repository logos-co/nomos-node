// std
use std::hash::{Hash, Hasher};
// crates
use nomos_core::da::blob::{self, metadata::Next};
use serde::{Deserialize, Serialize};
// internal

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct BlobInfo {
    id: [u8; 32],
    metadata: Metadata,
}

impl blob::info::DispersedBlobInfo for BlobInfo {
    type BlobId = [u8; 32];

    fn blob_id(&self) -> Self::BlobId {
        self.id
    }

    fn size(&self) -> usize {
        std::mem::size_of_val(&self.id) + self.metadata.size()
    }
}

impl Hash for BlobInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(<BlobInfo as blob::info::DispersedBlobInfo>::blob_id(self).as_ref());
    }
}

impl blob::metadata::Metadata for BlobInfo {
    type AppId = [u8; 32];
    type Index = Index;

    fn metadata(&self) -> (Self::AppId, Self::Index) {
        (self.metadata.app_id, self.metadata.index)
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, PartialOrd, Ord, Eq, Serialize, Deserialize)]
pub struct Index([u8; 8]);

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct Metadata {
    app_id: [u8; 32],
    index: Index,
}

impl Metadata {
    pub fn size(&self) -> usize {
        std::mem::size_of_val(&self.app_id) + std::mem::size_of_val(&self.index)
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

    use super::{Certificate, CertificateVerificationParameters, Index, VidCertificate};
    use crate::common::build_attestation_message;
    use blst::min_sig::SecretKey;
    use nomos_core::da::DaEncoder as _;
    use rand::{thread_rng, RngCore};
    use crate::{
        common::blob::DaBlob,
        encoder::{
            test::{rand_data, ENCODER},
            EncodedData,
        },
        verifier::DaVerifier,
    };
    use bitvec::prelude::*;
    use blst::min_sig::{PublicKey, SecretKey};
    use kzgrs::KzgRsError;
    use nomos_core::da::certificate::metadata::{Metadata as TraitMetadata, Next};
    use nomos_core::da::certificate::vid::VidCertificate as TraitVidCertificate;
    use nomos_core::da::certificate::Certificate as TraitCertificate;
    use rand::{rngs::OsRng, thread_rng, Rng, RngCore};
    use std::hash::{DefaultHasher, Hash};

    fn generate_keys() -> (PublicKey, SecretKey) {
        let mut rng = OsRng;
        let sk_bytes: [u8; 32] = rng.gen();
        let sk = SecretKey::key_gen(&sk_bytes, &[]).unwrap();
        let pk = sk.sk_to_pk();
        (pk, sk)
    }

    fn attest_encoded_data(encoded_data: &EncodedData, verifiers: &[DaVerifier]) -> Vec<bool> {
        let mut attestations = Vec::new();
        let domain_size = encoded_data.extended_data.0[0].len();
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
            attestations.push(verifier.verify(&da_blob, domain_size));
        }
        attestations
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

        let attestations = attest_encoded_data(&encoded_data, &verifiers);
        
        assert!(!attestations.contains(&false));
      
        let signers = bitvec![u8, Lsb0; 1; 16];
        let cert = Certificate::build_certificate(
            &encoded_data,
            &attestations,
            signers.clone(),
            THRESHOLD,
            Metadata::default(),
        )
        .unwrap();

        let public_keys: Vec<PublicKey> = sks.iter().map(|sk| sk.sk_to_pk()).collect();
        assert!(cert.verify(&public_keys));

        // Test AttestationSignersMismatch error case
        let new_attestations = [attestations.clone(), attestations.clone()].concat();

        let cert_result = Certificate::build_certificate(
            &encoded_data,
            &new_attestations,
            signers.clone(),
            THRESHOLD,
            Metadata::default(),
        );

        match cert_result {
            Err(KzgRsError::AttestationSignersMismatch {
                attestations_count,
                signers_count,
            }) if attestations_count == 32 && signers_count == 16 => {}
            _ => panic!("error does not match the KzgRsError::AttestationSignersMismatch pattern"),
        };

        // Test NotEnoughAttestations error case
        let cert_result = Certificate::build_certificate(
            &encoded_data,
            &attestations,
            signers,
            2 * THRESHOLD,
            Metadata::default(),
        );

        match cert_result {
            Err(KzgRsError::NotEnoughAttestations { required, received })
                if required == 32 && received == 16 => {}
            _ => panic!("error does not match the KzgRsError::NotEnoughAttestations pattern"),
        };
    }

    #[test]
    fn test_encoded_data_insufficient_verification() {
        const THRESHOLD: usize = 16;

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

        let mut attestations = attest_encoded_data(&encoded_data, &verifiers);

        // Imitate missing attestation.
        attestations.pop();

        let signers = bitvec![u8, Lsb0; 1; 16];
        let cert_result = Certificate::build_certificate(
            &encoded_data,
            &attestations,
            signers,
            THRESHOLD,
            Metadata::default(),
        );

        // Certificate won't be created because of not reaching required threshold.
        assert!(cert_result.is_err());
    }

    #[test]
    fn test_encoded_data_wrong_pk_verification() {
        const THRESHOLD: usize = 16;

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

        let attestations = attest_encoded_data(&encoded_data, &verifiers);

        let signers = bitvec![u8, Lsb0; 1; 16];
        let cert = Certificate::build_certificate(
            &encoded_data,
            &attestations,
            signers,
            THRESHOLD,
            Metadata::default(),
        )
        .unwrap();

        let mut public_keys: Vec<PublicKey> = sks.iter().map(|sk| sk.sk_to_pk()).collect();

        // Imitate different set of public keys on the verifier side.
        let (wrong_pk, _) = generate_keys();
        public_keys.pop();
        public_keys.push(wrong_pk);

        // Certificate should fail to be verified.
        assert!(!cert.verify(&public_keys));

    }

    #[test]
    fn test_certificate_methods() {
        const THRESHOLD: usize = 16;

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

        let attestations = attest_encoded_data(&encoded_data, &verifiers);

        let signers = bitvec![u8, Lsb0; 1; 16];
        let cert = Certificate::build_certificate(
            &encoded_data,
            &attestations,
            signers.clone(),
            THRESHOLD,
            Metadata::default(),
        )
        .unwrap();

        let public_keys: Vec<PublicKey> = sks.iter().map(|sk| sk.sk_to_pk()).collect();
        assert!(cert.verify(&public_keys));

        // Test generate hash for Certificate
        let mut default_hasher = DefaultHasher::new();
        let _hash = <Certificate as Hash>::hash(&cert, &mut default_hasher);

        // Test get signers
        let signers2 = cert.signers();
        let signers: Vec<bool> = signers.iter().map(|b| *b).collect();
        assert_eq!(signers2, signers);

        // Test get signature
        let signature2 = cert.signature();
        assert_eq!(signature2, cert.aggregated_signatures);

        // Test get id
        assert_eq!(
            cert.id(),
            build_attestation_message(&cert.aggregated_column_commitment, &cert.row_commitments)
        );

        // Test verify
        let params = CertificateVerificationParameters {
            nodes_public_keys: public_keys,
        };
        assert!(<Certificate as TraitCertificate>::verify(&cert, params));

        // Check metadata size
        let metadata_size2 = cert.metadata.size();
        let metadata = Metadata::default();
        let metadata_size =
            std::mem::size_of_val(&metadata.app_id) + std::mem::size_of_val(&metadata.index);
        assert_eq!(metadata_size2, metadata_size);

        // Create VidCertificate
        let vid_cert = VidCertificate::from(cert.clone());

        // Test generate hash for VidCertificate
        let mut default_hasher = DefaultHasher::new();
        let _hash = <VidCertificate as Hash>::hash(&vid_cert, &mut default_hasher);

        // Test get id
        assert_eq!(vid_cert.certificate_id(), vid_cert.id);

        // Check VidCertificate size
        let vid_cert_size2 = vid_cert.size();
        let vid_cert_size = std::mem::size_of_val(&vid_cert.id) + metadata_size;
        assert_eq!(vid_cert_size2, vid_cert_size);

        // Test get metadata for VidCertificate
        let (vid_app_id2, vid_index2) = vid_cert.metadata();
        let orig_app_id = cert.metadata.app_id;
        let orig_index = cert.metadata.index;
        assert_eq!(vid_app_id2, orig_app_id);
        assert_eq!(vid_index2, orig_index);

        // Create Index and test get next Index method
        let index = Index::from(32u64);
        let next_index2 = index.next();

        let num = u64::from_be_bytes(index.0);
        let incremented_num = num.wrapping_add(1);
        let next_index = Index(incremented_num.to_be_bytes());

        assert_eq!(next_index2.as_ref(), next_index.as_ref());
    }
}
