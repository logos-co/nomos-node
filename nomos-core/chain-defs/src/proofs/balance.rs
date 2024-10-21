// std
// crates
use risc0_zkvm::Prover;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
// internal
use nomos_proof_statements::bundle::{BundlePrivate, BundlePublic};

#[derive(Debug, Clone)]
pub struct BalanceProof {
    proof: risc0_zkvm::Receipt,
}

impl BalanceProof {
    pub(crate) fn public_inputs(&self) -> Result<BundlePublic, risc0_zkvm::serde::Error> {
        self.proof.journal.decode()
    }

    pub(crate) fn prove(
        bundle_witness: &cl::BundleWitness,
        prover: &dyn Prover,
    ) -> Result<Self, anyhow::Error> {
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
        let prove_info =
            prover.prove_with_opts(env, nomos_risc0_proofs::BUNDLE_BALANCE_ELF, &opts)?;

        tracing::trace!(
            "STARK 'bundle' prover time: {:.2?}, total_cycles: {}",
            start_t.elapsed(),
            prove_info.stats.total_cycles
        );

        Ok(Self {
            proof: prove_info.receipt,
        })
    }
}

impl Serialize for BalanceProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.proof.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BalanceProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let proof = risc0_zkvm::Receipt::deserialize(deserializer)?;
        proof
            .verify(nomos_risc0_proofs::BUNDLE_BALANCE_ID)
            .map_err(serde::de::Error::custom)?;

        Ok(Self { proof })
    }
}
