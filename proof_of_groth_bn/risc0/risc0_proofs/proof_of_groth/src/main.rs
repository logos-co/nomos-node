use ark_bn254::{Bn254, Fq, Fq2, Fr};
use ark_ff::PrimeField;
use ark_groth16::Proof;
use risc0_zkvm::guest::env;
use std::str::FromStr;

const G1_SIZE: usize = 32;


fn main() {

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
    eprintln!("DONE");

    let start_start = env::cycle_count();

    let mut inputs = vec![0u8; 8 * G1_SIZE + 32];
    env::read_slice(&mut inputs);
    let end = env::cycle_count();
    eprintln!("input load: {}", end - start_start);

    let start = env::cycle_count();
    let proof: Proof<Bn254> = Proof {
        a: ark_bn254::G1Affine::new(
            Fq::from_le_bytes_mod_order(&inputs[0..G1_SIZE]),
            Fq::from_le_bytes_mod_order(&inputs[G1_SIZE..2 * G1_SIZE]),
        ),
        b: ark_bn254::G2Affine::new(
            Fq2::new(
                Fq::from_le_bytes_mod_order(&inputs[2 * G1_SIZE..3 * G1_SIZE]),
                Fq::from_le_bytes_mod_order(&inputs[3 * G1_SIZE..4 * G1_SIZE]),
            ),
            Fq2::new(
                Fq::from_le_bytes_mod_order(&inputs[4 * G1_SIZE..5 * G1_SIZE]),
                Fq::from_le_bytes_mod_order(&inputs[5 * G1_SIZE..6 * G1_SIZE]),
            ),
        ),
        c: ark_bn254::G1Affine::new(
            Fq::from_le_bytes_mod_order(&inputs[6 * G1_SIZE..7 * G1_SIZE]),
            Fq::from_le_bytes_mod_order(&inputs[7 * G1_SIZE..8 * G1_SIZE]),
        ),
    };
    let end = env::cycle_count();
    eprintln!("proof : {:?}",proof);
    eprintln!("proof conversion: {}", end - start);

    let start = env::cycle_count();
    let public_input = vec![Fr::from_le_bytes_mod_order(
        &inputs[8 * G1_SIZE..8 * G1_SIZE + 32],
    )];
    let end = env::cycle_count();
    eprintln!("public input conversion: {}", end - start);

    let start = env::cycle_count();
    // BLS scalar field modulus
    let test = ark_groth16::Groth16::<Bn254>::verify_proof(&verification_key, &proof, &public_input).unwrap();
    let end = env::cycle_count();
    eprintln!("proof verification: {}", end - start);

    let start = env::cycle_count();
    assert_eq!(test, true);
    let end_end = env::cycle_count();
    eprintln!("test bool: {}", end_end - start);
    eprintln!("total: {}", end_end - start_start);
}
