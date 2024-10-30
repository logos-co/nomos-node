// std
// crates
use risc0_zkvm::Prover;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
// internal
use cl::Covenant;
use nomos_proof_statements::covenant::CovenantPublic;

#[derive(Debug, Clone)]
pub struct CovenantProof {
    pub risc0_id: [u32; 8],
    pub proof: risc0_zkvm::Receipt,
}

pub fn risc0_covenant(risc0_id: [u32; 8]) -> Covenant {
    // Commit to a RISC0 ID for use as a note constraint

    let mut bytes = [0u8; 32];

    for (i, word) in risc0_id.iter().enumerate() {
        let word_bytes = word.to_le_bytes();
        bytes[i * 4] = word_bytes[0];
        bytes[i * 4 + 1] = word_bytes[1];
        bytes[i * 4 + 2] = word_bytes[2];
        bytes[i * 4 + 3] = word_bytes[3];
    }

    Covenant::from_vk(&bytes)
}

impl CovenantProof {
    pub fn covenant(&self) -> Covenant {
        risc0_covenant(self.risc0_id)
    }

    pub(crate) fn public_inputs(&self) -> Result<CovenantPublic, risc0_zkvm::serde::Error> {
        self.proof.journal.decode()
    }

    pub fn nop_constraint() -> Covenant {
        risc0_covenant(nomos_risc0_proofs::COVENANT_NOP_ID)
    }

    pub fn prove_nop(
        nf: cl::Nullifier,
        ptx_root: cl::PtxRoot,
        prover: &dyn Prover,
    ) -> Result<Self, anyhow::Error> {
        let constraint_public = CovenantPublic { nf, ptx_root };
        let env = risc0_zkvm::ExecutorEnv::builder()
            .write(&constraint_public)?
            .build()?;
        let start_t = std::time::Instant::now();

        let opts = risc0_zkvm::ProverOpts::groth16();
        let prove_info =
            prover.prove_with_opts(env, nomos_risc0_proofs::COVENANT_NOP_ELF, &opts)?;

        tracing::trace!(
            "STARK 'constraint-nop' prover time: {:.2?}, total_cycles: {}",
            start_t.elapsed(),
            prove_info.stats.total_cycles
        );

        Ok(Self {
            risc0_id: nomos_risc0_proofs::COVENANT_NOP_ID,
            proof: prove_info.receipt,
        })
    }
}

impl Serialize for CovenantProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.risc0_id, &self.proof).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CovenantProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (risc0_id, proof) = <([u32; 8], risc0_zkvm::Receipt)>::deserialize(deserializer)?;
        proof.verify(risc0_id).map_err(serde::de::Error::custom)?;

        Ok(Self { risc0_id, proof })
    }
}
