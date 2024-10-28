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
            "639333940093023186600448085366851845273737392353332031120714371691414309877750194052115886384057229583143737603997",
        )
        .unwrap()
        .to_bytes_be(),
    ));
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "1345735295075288476376674445029774826073266825737456288226707851660585050553004334167772938381912095772690260920733",
        )
        .unwrap()
        .to_bytes_be(),
    ));
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "614513702250363818762226897076669319783415621696223855465238002798073028758712586531122663262971154006221475164723",
        )
        .unwrap()
        .to_bytes_be(),
    ));
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "1125983982403509485044589117359732524753360034519375478944173458194614491211736082676147223664196387864570831687718",
        )
        .unwrap()
        .to_bytes_be(),
    ));
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "1687394049980889418911067727625204832660166634458979919513232185682738456465333188373123600105728170601224657373197",
        )
        .unwrap()
        .to_bytes_be(),
    ));
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "2848163681488550236055436814424157633548590450375389536968924298779254970715722204533103640391891896467748944852203",
        )
        .unwrap()
        .to_bytes_be(),
    ));
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "750428978006537666419012627200114038716132207148493980572574050194449789841887987018980434421287837061549760141183",
        )
        .unwrap()
        .to_bytes_be(),
    ));
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "2202993282647029893333122592614636721719950262131300127878843739831674365357517493542436362753424753741462804625654",
        )
        .unwrap()
        .to_bytes_be(),
    ));
    buffer.append(&mut Vec::from(
        BigUint::from_str(
            "907856803541436205028955228894831559056854519137108240336065231664534533428",
        )
            .unwrap()
            .to_bytes_be(),
    ));
    let env = risc0_zkvm::ExecutorEnv::builder()
        .write_slice(&buffer)
        .build()
        .unwrap();

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
