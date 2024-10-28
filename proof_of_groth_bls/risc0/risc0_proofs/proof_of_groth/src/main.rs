use ark_bls12_381::{Bls12_381, Fq, Fq2, Fr};
use ark_ff::PrimeField;
use ark_groth16::Proof;
use risc0_zkvm::guest::env;
use std::str::FromStr;

const G1_SIZE: usize = 48;


fn main() {

    let verification_key: ark_groth16::PreparedVerifyingKey<Bls12_381> = ark_groth16::verifier::prepare_verifying_key(&ark_groth16::VerifyingKey {
        alpha_g1: ark_bls12_381::G1Affine::new(
            Fq::from_str(
                "619059981246652214113808957975475215153915552337154345605473229410663539424051356776882471634458034445128231065670"
            )
                    .unwrap(),
                Fq::from_str(
                    "3608316669558676154234890964099873349281005844944715017494950726357608086612920113070153507629789270476690975942546"
                )
                    .unwrap(),
            ),
        beta_g2: ark_bls12_381::G2Affine::new(
                Fq2::new(
                    Fq::from_str(
                        "3365743120932353577092174838345855807932517155696109764749010409720811856269911935138999550669725041993203722515248",
                    )
                        .unwrap(),
                    Fq::from_str(
                        "339153471736178528466979464445212535857553626851531311109950732328443782657329785545726953968749408247886086228887",
                    )
                        .unwrap(),
                ),
                Fq2::new(
                    Fq::from_str(
                        "3601050428708681028521223746342984111738959792503556226260504628534713618932852870117840660682276764750826725799052",
                    )
                        .unwrap(),
                    Fq::from_str(
                        "3655708416990633870612035441050900895142763487488484865111088634142859108059930184923947404845378912851440663185560",
                    )
                        .unwrap(),
                ),
            ),
        gamma_g2: ark_bls12_381::G2Affine::new(
                Fq2::new(
                    Fq::from_str(
                        "352701069587466618187139116011060144890029952792775240219908644239793785735715026873347600343865175952761926303160",
                    )
                        .unwrap(),
                    Fq::from_str(
                        "3059144344244213709971259814753781636986470325476647558659373206291635324768958432433509563104347017837885763365758",
                    )
                        .unwrap(),
                ),
                Fq2::new(
                    Fq::from_str(
                        "1985150602287291935568054521177171638300868978215655730859378665066344726373823718423869104263333984641494340347905",
                    )
                        .unwrap(),
                    Fq::from_str(
                        "927553665492332455747201965776037880757740193453592970025027978793976877002675564980949289727957565575433344219582",
                    )
                        .unwrap(),
                ),
            ),
        delta_g2: ark_bls12_381::G2Affine::new(
                Fq2::new(
                    Fq::from_str(
                        "2569106892729929968803638884494166937615193344396159696309091364261105174622246844730715943934916982554680516563656",
                    )
                        .unwrap(),
                    Fq::from_str(
                        "2467780249368270772287289714608024544923361825227593643072439623209932041521780350034009574818606209922302862796904",
                    )
                        .unwrap(),
                ),
                Fq2::new(
                    Fq::from_str(
                        "2050475360089110796236495666665504374311381084599319206111520860498306593907129029112106291414495338362429827313921",
                    )
                        .unwrap(),
                    Fq::from_str(
                        "1190286647705113237739730925123157988259651418087385964553961849059606752089056975915896742770723662211212141823486",
                    )
                        .unwrap(),
                ),
        ),
        gamma_abc_g1: vec![
                ark_bls12_381::G1Affine::new(
                    Fq::from_str(
                        "3909493174197355863781768009134818424343123496372905284242107516362677541173739292406858135508207077322923851722416",
                    )
                        .unwrap(),
                    Fq::from_str(
                        "2677964957875335544371804669771387809913312256382301444955850551858207340362786241404226710658693668163795601435388",
                    )
                        .unwrap(),
                ),
                ark_bls12_381::G1Affine::new(
                    Fq::from_str(
                        "2378701639209403174421160413975884933277979566646799315457416610937936884712120615995796608516102383547291737171294",
                    )
                        .unwrap(),
                    Fq::from_str(
                        "915854573668701530379971847511909332359228444006146143133737553737546964197468047771278625590651681607368808182274",
                    )
                        .unwrap(),
                )
            ]
    });

    let start_start = env::cycle_count();

    let mut inputs = vec![0u8; 8 * G1_SIZE + 32];
    env::read_slice(&mut inputs);
    let end = env::cycle_count();
    eprintln!("input load: {}", end - start_start);

    let start = env::cycle_count();
    let proof: Proof<Bls12_381> = Proof {
        a: ark_bls12_381::G1Affine::new(
            Fq::from_be_bytes_mod_order(&inputs[0..G1_SIZE]),
            Fq::from_be_bytes_mod_order(&inputs[G1_SIZE..2 * G1_SIZE]),
        ),
        b: ark_bls12_381::G2Affine::new(
            Fq2::new(
                Fq::from_be_bytes_mod_order(&inputs[2 * G1_SIZE..3 * G1_SIZE]),
                Fq::from_be_bytes_mod_order(&inputs[3 * G1_SIZE..4 * G1_SIZE]),
            ),
            Fq2::new(
                Fq::from_be_bytes_mod_order(&inputs[4 * G1_SIZE..5 * G1_SIZE]),
                Fq::from_be_bytes_mod_order(&inputs[5 * G1_SIZE..6 * G1_SIZE]),
            ),
        ),
        c: ark_bls12_381::G1Affine::new(
            Fq::from_be_bytes_mod_order(&inputs[6 * G1_SIZE..7 * G1_SIZE]),
            Fq::from_be_bytes_mod_order(&inputs[7 * G1_SIZE..8 * G1_SIZE]),
        ),
    };
    let end = env::cycle_count();
    eprintln!("proof : {:?}",proof);
    eprintln!("proof conversion: {}", end - start);

    let start = env::cycle_count();
    let public_input = vec![Fr::from_be_bytes_mod_order(
        &inputs[8 * G1_SIZE..8 * G1_SIZE + 32],
    )];
    let end = env::cycle_count();
    eprintln!("public input conversion: {}", end - start);

    let start = env::cycle_count();
    // BLS scalar field modulus
    let test = ark_groth16::Groth16::<Bls12_381>::verify_proof(&verification_key, &proof, &public_input).unwrap();
    let end = env::cycle_count();
    eprintln!("proof verification: {}", end - start);

    let start = env::cycle_count();
    assert_eq!(test, true);
    let end_end = env::cycle_count();
    eprintln!("test bool: {}", end_end - start);
    eprintln!("total: {}", end_end - start_start);
}
