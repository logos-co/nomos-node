// std
//crates
use bytes::{Bytes, BytesMut};
use risc0_zkvm::Prover;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
//internal
use super::Error;
use crate::wire;
use nomos_proof_statements::bundle::{BundlePrivate, BundlePublic};

#[derive(Debug, Clone)]
pub struct Bundle {
    bundle: cl::Bundle,
    //TODO: this can be pruned once validated
    proof: risc0_zkvm::Receipt,
}

impl Bundle {
    pub fn bundle(&self) -> &cl::Bundle {
        &self.bundle
    }

    pub fn prove(bundle_witness: &cl::BundleWitness, prover: &dyn Prover) -> Result<Self, Error> {
        // need to show that bundle is balanced.
        // i.e. the sum of ptx balances is 0
        let bundle_private = BundlePrivate {
            balances: bundle_witness
                .partial_witnesses()
                .iter()
                .map(|ptx| ptx.balance())
                .collect(),
        };

        let env = risc0_zkvm::ExecutorEnv::builder()
            .write(&bundle_private)?
            .build()?;

        let start_t = std::time::Instant::now();

        let opts = risc0_zkvm::ProverOpts::groth16();
        let prove_info = prover.prove_with_opts(env, nomos_cl_risc0_proofs::BUNDLE_ELF, &opts)?;

        tracing::trace!(
            "STARK 'bundle' prover time: {:.2?}, total_cycles: {}",
            start_t.elapsed(),
            prove_info.stats.total_cycles
        );

        let receipt = prove_info.receipt;

        Ok(Self {
            bundle: bundle_witness.commit(),
            proof: receipt,
        })
    }

    pub fn as_bytes(&self) -> Bytes {
        let mut buff = BytesMut::new();
        wire::serializer_into_buffer(&mut buff)
            .serialize_into(&self)
            .expect("MockTransaction serialization to buffer failed");
        buff.freeze()
    }
}

impl Serialize for Bundle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.bundle, &self.proof).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Bundle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // TODO: limit number of bites serialized
        let (bundle, proof) = <(cl::Bundle, risc0_zkvm::Receipt)>::deserialize(deserializer)?;

        proof
            .verify(nomos_cl_risc0_proofs::BUNDLE_ID)
            .map_err(D::Error::custom)?;

        let bundle_public: BundlePublic = proof.journal.decode().map_err(D::Error::custom)?;
        if Vec::from_iter(bundle.partial_txs().iter().map(|ptx| ptx.balance))
            != bundle_public.balances
        {
            return Err(D::Error::custom("Bundle balance mismatch"));
        }

        Ok(Self { bundle, proof })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cl::{note::derive_unit, *};

    fn receive_utxo(note: cl::NoteWitness, nf_pk: cl::NullifierCommitment) -> cl::OutputWitness {
        cl::OutputWitness::new(note, nf_pk)
    }

    #[test]
    fn test_bundle() {
        let nmo = derive_unit("NMO");
        let mut rng = rand::thread_rng();

        let sender_nf_sk = cl::NullifierSecret::random(&mut rng);
        let sender_nf_pk = sender_nf_sk.commit();

        let recipient_nf_pk = cl::NullifierSecret::random(&mut rng).commit();

        // Assume the sender has received an unspent output from somewhere
        let utxo = receive_utxo(cl::NoteWitness::basic(10, nmo, &mut rng), sender_nf_pk);

        // and wants to send 8 NMO to some recipient and return 2 NMO to itself.
        let recipient_output =
            cl::OutputWitness::new(cl::NoteWitness::basic(8, nmo, &mut rng), recipient_nf_pk);
        let change_output =
            cl::OutputWitness::new(cl::NoteWitness::basic(2, nmo, &mut rng), sender_nf_pk);

        let ptx_witness = cl::PartialTxWitness {
            inputs: vec![cl::InputWitness::from_output(utxo, sender_nf_sk)],
            outputs: vec![recipient_output, change_output],
            balance_blinding: BalanceWitness::random_blinding(&mut rng),
        };

        let bundle = cl::BundleWitness::new(vec![ptx_witness]);

        // ATTENTION: building a valid proof requires a x86 machine with docker installed
        // if you don't have one, you can run this test with RISC0_DEV_MODE=1 or skip the test
        let prover = risc0_zkvm::default_prover();
        let bundle = super::Bundle::prove(&bundle, prover.as_ref()).unwrap();

        assert_eq!(
            wire::serialize(&bundle).unwrap(),
            wire::serialize(
                &wire::deserialize::<super::Bundle>(&wire::serialize(&bundle).unwrap()).unwrap()
            )
            .unwrap()
        );
    }
}
