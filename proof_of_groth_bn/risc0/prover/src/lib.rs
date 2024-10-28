use num_bigint::BigUint;
use risc0_zkvm::Receipt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to produce proof")]
    ProofError(#[from] anyhow::Error),
}

pub fn prove() -> Result<Receipt, Error> {
    let mut buffer: Vec<u8> = vec![];
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "18535920750860255797837490390844110873820661778575217648309528321689628605871",
        )
        .unwrap()
        .to_bytes_le(),
    ));
    for _i in buffer.len()..32 {
        buffer.append(&mut vec![0u8]);
    }
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "16675115375364837468266925426616459574763592003721600779895729975058274290182",
        )
        .unwrap()
        .to_bytes_le(),
    ));
    for _i in buffer.len()..64 {
        buffer.append(&mut vec![0u8]);
    }
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "2484839624523956430373012747226443549930942897555011186301588478091263270002",
        )
        .unwrap()
        .to_bytes_le(),
    ));
    for _i in buffer.len()..96 {
        buffer.append(&mut vec![0u8]);
    }
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "12370405754194290571280041285758564472244573429702930903967524682733272076088",
        )
        .unwrap()
        .to_bytes_le(),
    ));
    for _i in buffer.len()..128 {
        buffer.append(&mut vec![0u8]);
    }
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "197851975532637812742092225699201104908392250564004752327778316922874240094",
        )
        .unwrap()
        .to_bytes_le(),
    ));
    for _i in buffer.len()..160 {
        buffer.append(&mut vec![0u8]);
    }
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "12994375192183627971631049964557214775338278780210956566789520934230576292766",
        )
        .unwrap()
        .to_bytes_le(),
    ));
    for _i in buffer.len()..192 {
        buffer.append(&mut vec![0u8]);
    }
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "6967804814242303615387035056751825308494195011941147704979964946252946806847",
        )
        .unwrap()
        .to_bytes_le(),
    ));
    for _i in buffer.len()..224 {
        buffer.append(&mut vec![0u8]);
    }
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "7960927355537035004276257617922445025092429737051371911092442601691534705070",
        )
        .unwrap()
        .to_bytes_le(),
    ));
    for _i in buffer.len()..256 {
        buffer.append(&mut vec![0u8]);
    }
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "18586133768512220936620570745912940619677854269274689475585506675881198879027",
        )
            .unwrap()
            .to_bytes_le(),
    ));
    for _i in buffer.len()..288 {
        buffer.append(&mut vec![0u8]);
    }
    let env = risc0_zkvm::ExecutorEnv::builder()
        .write_slice(&buffer)
        .build()
        .unwrap();
    println!("buffer size: {}", buffer.len());

    // Obtain the default prover.
    let prover = risc0_zkvm::default_prover();

    let start_t = std::time::Instant::now();

    // Proof information by proving the specified ELF binary.
    // This struct contains the receipt along with statistics about execution of the guest
    let opts = risc0_zkvm::ProverOpts::succinct();
    let prove_info =
        prover.prove_with_opts(env, nomos_groth_risc0_proofs::PROOF_OF_GROTH_ELF, &opts)?;

    println!(
        "STARK prover time: {:.2?}, total_cycles: {}",
        start_t.elapsed(),
        prove_info.stats.total_cycles
    );
    // extract the receipt.
    Ok(prove_info.receipt)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_groth_prover() {
        let proof = prove().unwrap();
        assert!(proof
            .verify(nomos_groth_risc0_proofs::PROOF_OF_GROTH_ID)
            .is_ok());
    }
}
