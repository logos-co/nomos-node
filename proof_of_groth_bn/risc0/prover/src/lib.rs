use risc0_zkvm::Receipt;
use std::str::FromStr;
use ark_bn254::{Bn254, Fq, Fq2, Fr};
use thiserror::Error;
use ark_groth16;
use ark_groth16::Proof;
use ark_serialize::CanonicalSerialize;

#[derive(Error, Debug)]
pub enum Error {
    #[error("failed to produce proof")]
    ProofError(#[from] anyhow::Error),
}

pub fn prove() -> Result<Receipt, Error> {
    let verification_key: ark_groth16::PreparedVerifyingKey<Bn254> = ark_groth16::verifier::prepare_verifying_key(&ark_groth16::VerifyingKey {
        alpha_g1: ark_bn254::G1Affine::new(
            Fq::from_str(
                "251227729391918740328351366986482163538561086137222016477254145870412353529"
            )
                .unwrap(),
            Fq::from_str(
                "559134868112755803428812495974418565931542101366334011081334240262863755982"
            )
                .unwrap(),
        ),
        beta_g2: ark_bn254::G2Affine::new(
            Fq2::new(
                Fq::from_str(
                    "1384591907268489663157825649775769772874486359597097187170060951282624040471",
                )
                    .unwrap(),
                Fq::from_str(
                    "20527186200207158422542054126592395590304798705061350246552840056437826919471",
                )
                    .unwrap(),
            ),
            Fq2::new(
                Fq::from_str(
                    "19073888111152448992815596675541050741473786354758111546109259642686146139411",
                )
                    .unwrap(),
                Fq::from_str(
                    "20778976629983482863111101101623211010134366101756784673744371706255058976522",
                )
                    .unwrap(),
            ),
        ),
        gamma_g2: ark_bn254::G2Affine::new(
            Fq2::new(
                Fq::from_str(
                    "10857046999023057135944570762232829481370756359578518086990519993285655852781",
                )
                    .unwrap(),
                Fq::from_str(
                    "11559732032986387107991004021392285783925812861821192530917403151452391805634",
                )
                    .unwrap(),
            ),
            Fq2::new(
                Fq::from_str(
                    "8495653923123431417604973247489272438418190587263600148770280649306958101930",
                )
                    .unwrap(),
                Fq::from_str(
                    "4082367875863433681332203403145435568316851327593401208105741076214120093531",
                )
                    .unwrap(),
            ),
        ),
        delta_g2: ark_bn254::G2Affine::new(
            Fq2::new(
                Fq::from_str(
                    "2382770829579450231937877595813135214627755341163308338559481788618722905625",
                )
                    .unwrap(),
                Fq::from_str(
                    "9040926302247894040188704482087249224621680323614760223290765369622797702873",
                )
                    .unwrap(),
            ),
            Fq2::new(
                Fq::from_str(
                    "21113171019167140870581514687281577061289301731643345706699116515401076091323",
                )
                    .unwrap(),
                Fq::from_str(
                    "3201874119644369618625632439639166311277319924024234206341116264282425882294",
                )
                    .unwrap(),
            ),
        ),
        gamma_abc_g1: vec![
            ark_bn254::G1Affine::new(
                Fq::from_str(
                    "7310956019542048104134504823116852886290432062723958243364678445169725760527",
                )
                    .unwrap(),
                Fq::from_str(
                    "20578404810367721733140279428999447068198116990404418250099811286304698556813",
                )
                    .unwrap(),
            ),
            ark_bn254::G1Affine::new(
                Fq::from_str(
                    "737015672572854077167569176360882043824770055311314595207480035063025978170",
                )
                    .unwrap(),
                Fq::from_str(
                    "2704689443166500407886938474276433769995428448878740741961672111817265043228",
                )
                    .unwrap(),
            )
        ]
    });
    let mut vec_verif_key: Vec<u8> = vec![];
    verification_key.serialize_compressed(&mut vec_verif_key).unwrap();

    let proof: Proof<Bn254> = Proof {
        a: ark_bn254::G1Affine::new(
            Fq::from_str(
                "18535920750860255797837490390844110873820661778575217648309528321689628605871",
            )
                .unwrap(),
            Fq::from_str(
                "16675115375364837468266925426616459574763592003721600779895729975058274290182",
            )
                .unwrap(),
        ),
        b: ark_bn254::G2Affine::new(
            Fq2::new(
                Fq::from_str(
                    "2484839624523956430373012747226443549930942897555011186301588478091263270002",
                )
                    .unwrap(),
                Fq::from_str(
                    "12370405754194290571280041285758564472244573429702930903967524682733272076088",
                )
                    .unwrap(),
            ),
            Fq2::new(
                Fq::from_str(
                    "197851975532637812742092225699201104908392250564004752327778316922874240094",
                )
                    .unwrap(),
                Fq::from_str(
                    "12994375192183627971631049964557214775338278780210956566789520934230576292766",
                )
                    .unwrap(),
            ),
        ),
        c: ark_bn254::G1Affine::new(
            Fq::from_str(
                "6967804814242303615387035056751825308494195011941147704979964946252946806847",
            )
                .unwrap(),
            Fq::from_str(
                "7960927355537035004276257617922445025092429737051371911092442601691534705070",
            )
                .unwrap(),
        ),
    };
    let mut vec_proof: Vec<u8> = vec![];
    proof.serialize_compressed(&mut vec_proof).unwrap();


    let public = vec![Fr::from_str("18586133768512220936620570745912940619677854269274689475585506675881198879027").unwrap()];
    let mut vec_public: Vec<u8> = vec![];
    public.serialize_compressed(&mut vec_public).unwrap();

    let mut buffer: Vec<u8> = vec![];
    buffer.append(&mut vec_verif_key);
    buffer.append(&mut vec_proof);
    buffer.append(&mut vec_public);

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
