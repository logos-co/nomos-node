// std
use std::{
    sync::{
        atomic::{AtomicBool, Ordering::SeqCst},
        Arc,
    },
    time::Duration,
};
// crates
use cl::{InputWitness, NoteWitness, NullifierSecret};
use cryptarchia_consensus::TimeConfig;
use cryptarchia_ledger::{Coin, LedgerState};
use kzgrs_backend::{
    common::blob::DaBlob,
    encoder::{DaEncoder, DaEncoderParams},
};
use nomos_core::da::DaEncoder as _;
use nomos_da_network_service::NetworkConfig as DaNetworkConfig;
use nomos_da_network_service::NetworkService as DaNetworkService;
use nomos_da_sampling::backend::kzgrs::KzgrsSamplingBackendSettings;
use nomos_da_sampling::network::adapters::libp2p::DaNetworkSamplingSettings;
use nomos_da_sampling::storage::adapters::rocksdb::RocksAdapterSettings as SamplingStorageSettings;
use nomos_da_sampling::DaSamplingServiceSettings;
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifierSettings;
use nomos_libp2p::SwarmConfig;
use rand::{thread_rng, Rng};
use tempfile::{NamedTempFile, TempDir};
use time::OffsetDateTime;
// internal
use crate::common::*;

pub const PARAMS: DaEncoderParams = DaEncoderParams::default_with(2);
pub const ENCODER: DaEncoder = DaEncoder::new(PARAMS);

#[test]
fn test_verifier() {
    let performed_tx = Arc::new(AtomicBool::new(false));
    let performed_rx = performed_tx.clone();
    let is_success_tx = Arc::new(AtomicBool::new(false));
    let is_success_rx = is_success_tx.clone();

    let mut ids = vec![[0; 32]; 2];
    for id in &mut ids {
        thread_rng().fill(id);
    }

    let notes = ids
        .iter()
        .map(|&id| {
            let mut sk = [0; 16];
            sk.copy_from_slice(&id[0..16]);
            InputWitness::new(
                NoteWitness::basic(1, NMO_UNIT, &mut thread_rng()),
                NullifierSecret(sk),
            )
        })
        .collect::<Vec<_>>();
    let genesis_state = LedgerState::from_commitments(
        notes.iter().map(|n| n.note_commitment()),
        (ids.len() as u32).into(),
    );
    let ledger_config = cryptarchia_ledger::Config {
        epoch_stake_distribution_stabilization: 3,
        epoch_period_nonce_buffer: 3,
        epoch_period_nonce_stabilization: 4,
        consensus_config: cryptarchia_engine::Config {
            security_param: 10,
            active_slot_coeff: 0.9,
        },
    };
    let time_config = TimeConfig {
        slot_duration: Duration::from_secs(1),
        chain_start_time: OffsetDateTime::now_utc(),
    };

    let swarm_config1 = SwarmConfig {
        port: 7773,
        ..Default::default()
    };
    let swarm_config2 = SwarmConfig {
        port: 7774,
        ..Default::default()
    };

    let blobs_dir = TempDir::new().unwrap().path().to_path_buf();

    let (node1_sk, node1_pk) = generate_hex_keys();
    let (node2_sk, node2_pk) = generate_hex_keys();

    let client_zone = new_client(NamedTempFile::new().unwrap().path().to_path_buf());

    let node1 = new_node(
        &notes[0],
        &ledger_config,
        &genesis_state,
        &time_config,
        &swarm_config1,
        NamedTempFile::new().unwrap().path().to_path_buf(),
        &blobs_dir,
        vec![node_address(&swarm_config2)],
        KzgrsDaVerifierSettings {
            sk: node1_sk.clone(),
            nodes_public_keys: vec![node1_pk.clone(), node2_pk.clone()],
        },
    );

    let node2 = new_node(
        &notes[1],
        &ledger_config,
        &genesis_state,
        &time_config,
        &swarm_config2,
        NamedTempFile::new().unwrap().path().to_path_buf(),
        &blobs_dir,
        vec![node_address(&swarm_config1)],
        KzgrsDaVerifierSettings {
            sk: node2_sk,
            nodes_public_keys: vec![node1_pk, node2_pk],
        },
    );

    let node1_verifier = node1.handle().relay::<DaVerifier>();

    let node2_verifier = node2.handle().relay::<DaVerifier>();

    client_zone.spawn(async move {
        let node1_verifier = node1_verifier.connect().await.unwrap();
        let (node1_reply_tx, node1_reply_rx) = tokio::sync::oneshot::channel();

        let node2_verifier = node2_verifier.connect().await.unwrap();
        let (node2_reply_tx, node2_reply_rx) = tokio::sync::oneshot::channel();

        let verifiers = vec![
            (node1_verifier, node1_reply_tx),
            (node2_verifier, node2_reply_tx),
        ];

        // Encode data
        let encoder = &ENCODER;
        let data = rand_data(10);

        let encoded_data = encoder.encode(&data).unwrap();
        let columns: Vec<_> = encoded_data.extended_data.columns().collect();

        for (i, (verifier, reply_tx)) in verifiers.into_iter().enumerate() {
            let column = &columns[i];

            let da_blob = DaBlob {
                column: column.clone(),
                column_idx: i
                    .try_into()
                    .expect("Column index shouldn't overflow the target type"),
                column_commitment: encoded_data.column_commitments[i],
                aggregated_column_commitment: encoded_data.aggregated_column_commitment,
                aggregated_column_proof: encoded_data.aggregated_column_proofs[i],
                rows_commitments: encoded_data.row_commitments.clone(),
                rows_proofs: encoded_data
                    .rows_proofs
                    .iter()
                    .map(|proofs| proofs.get(i).cloned().unwrap())
                    .collect(),
            };

            verifier
                .send(nomos_da_verifier::DaVerifierMsg::AddBlob {
                    blob: da_blob,
                    reply_channel: reply_tx,
                })
                .await
                .unwrap();
        }

        // Wait for response from the verifier.
        let a1 = node1_reply_rx.await.unwrap();
        let a2 = node2_reply_rx.await.unwrap();

        if a1.is_some() && a2.is_some() {
            is_success_tx.store(true, SeqCst);
        }

        performed_tx.store(true, SeqCst);
    });

    while !performed_rx.load(SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(is_success_rx.load(SeqCst));
}
