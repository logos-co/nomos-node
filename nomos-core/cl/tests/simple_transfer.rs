use cl::{note::derive_unit, BalanceWitness};

fn receive_utxo(note: cl::NoteWitness, nf_pk: cl::NullifierCommitment) -> cl::OutputWitness {
    cl::OutputWitness::new(note, nf_pk)
}

#[test]
fn test_simple_transfer() {
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
        inputs: vec![cl::InputWitness::from_output(utxo, sender_nf_sk, vec![])],
        outputs: vec![recipient_output, change_output],
        balance_blinding: BalanceWitness::random_blinding(&mut rng),
    };

    let bundle = cl::BundleWitness::new(vec![ptx_witness]);

    assert!(bundle.balance().is_zero())
}
