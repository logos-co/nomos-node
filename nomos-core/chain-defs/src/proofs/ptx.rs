// std
// crates
use risc0_zkvm::Prover;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
// internal
use nomos_proof_statements::ptx::{PtxPrivate, PtxPublic};

#[derive(Debug, Clone)]
pub struct PtxProof {
    proof: risc0_zkvm::Receipt,
}

impl PtxProof {
    pub(crate) fn public_inputs(&self) -> Result<PtxPublic, risc0_zkvm::serde::Error> {
        self.proof.journal.decode()
    }

    pub(crate) fn prove(
        ptx_witness: &cl::PartialTxWitness,
        cm_root: [u8; 32],
        prover: &dyn Prover,
    ) -> Result<Self, anyhow::Error> {
        let ptx_private = PtxPrivate {
            ptx: ptx_witness.clone(),
            cm_root,
        };

        let env = risc0_zkvm::ExecutorEnv::builder()
            .write(&ptx_private)?
            .build()?;

        let start_t = std::time::Instant::now();

        let opts = risc0_zkvm::ProverOpts::groth16();
        let prove_info = prover.prove_with_opts(env, nomos_risc0_proofs::PTX_ELF, &opts)?;

        tracing::trace!(
            "STARK 'ptx' prover time: {:.2?}, total_cycles: {}",
            start_t.elapsed(),
            prove_info.stats.total_cycles
        );

        Ok(Self {
            proof: prove_info.receipt,
        })
    }
}

impl Serialize for PtxProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.proof.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PtxProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let proof = risc0_zkvm::Receipt::deserialize(deserializer)?;
        proof
            .verify(nomos_risc0_proofs::PTX_ID)
            .map_err(serde::de::Error::custom)?;

        Ok(Self { proof })
    }
}
