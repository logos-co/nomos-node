use std::{
    collections::{HashMap, HashSet},
    env,
    path::PathBuf,
    str::FromStr,
    time::Duration,
};

use nomos_libp2p::{ed25519, Multiaddr, PeerId};
use nomos_node::NomosDaMembership;
use once_cell::sync::Lazy;
use subnetworks_assignations::MembershipHandler;

use crate::get_available_port;

pub static GLOBAL_PARAMS_PATH: Lazy<String> = Lazy::new(|| {
    let relative_path = "./kzgrs/kzgrs_test_params";
    let current_dir = env::current_dir().expect("Failed to get current directory");
    current_dir
        .join(relative_path)
        .canonicalize()
        .expect("Failed to resolve absolute path")
        .to_string_lossy()
        .to_string()
});

#[derive(Clone)]
pub struct DaParams {
    pub subnetwork_size: usize,
    pub dispersal_factor: usize,
    pub num_samples: u16,
    pub num_subnets: u16,
    pub old_blobs_check_interval: Duration,
    pub blobs_validity_duration: Duration,
    pub global_params_path: String,
}

impl Default for DaParams {
    fn default() -> Self {
        Self {
            subnetwork_size: 2,
            dispersal_factor: 1,
            num_samples: 1,
            num_subnets: 2,
            old_blobs_check_interval: Duration::from_secs(5),
            blobs_validity_duration: Duration::from_secs(u64::MAX),
            global_params_path: GLOBAL_PARAMS_PATH.to_string(),
        }
    }
}

#[derive(Clone)]
pub struct GeneralDaConfig {
    pub node_key: ed25519::SecretKey,
    pub peer_id: PeerId,
    pub membership: NomosDaMembership,
    pub addresses: HashMap<PeerId, Multiaddr>,
    pub listening_address: Multiaddr,
    pub blob_storage_directory: PathBuf,
    pub global_params_path: String,
    pub verifier_sk: String,
    pub verifier_index: HashSet<u32>,
    pub num_samples: u16,
    pub num_subnets: u16,
    pub old_blobs_check_interval: Duration,
    pub blobs_validity_duration: Duration,
}

pub fn create_da_configs(ids: &[[u8; 32]], da_params: DaParams) -> Vec<GeneralDaConfig> {
    let mut node_keys = vec![];
    let mut peer_ids = vec![];
    let mut listening_addresses = vec![];

    for id in ids {
        let mut node_key_bytes = *id;
        let node_key = ed25519::SecretKey::try_from_bytes(&mut node_key_bytes)
            .expect("Failed to generate secret key from bytes");
        node_keys.push(node_key.clone());

        let peer_id = secret_key_to_peer_id(node_key);
        peer_ids.push(peer_id);

        let listening_address = Multiaddr::from_str(&format!(
            "/ip4/127.0.0.1/udp/{}/quic-v1",
            get_available_port(),
        ))
        .expect("Failed to create multiaddr");
        listening_addresses.push(listening_address);
    }

    let membership = NomosDaMembership::new(
        &peer_ids,
        da_params.subnetwork_size,
        da_params.dispersal_factor,
    );

    let addresses = build_da_peer_list(&peer_ids, &listening_addresses);

    ids.iter()
        .zip(node_keys)
        .enumerate()
        .map(|(i, (id, node_key))| {
            let blob_storage_directory = PathBuf::from(format!("/tmp/blob_storage_{}", i));
            let verifier_sk = blst::min_sig::SecretKey::key_gen(id, &[]).unwrap();
            let verifier_sk_bytes = verifier_sk.to_bytes();
            let peer_id = peer_ids[i];

            let subnetwork_ids = membership.membership(&peer_id);

            GeneralDaConfig {
                node_key: node_key.clone(),
                peer_id,
                membership: membership.clone(),
                addresses: addresses.clone(),
                listening_address: listening_addresses[i].clone(),
                blob_storage_directory,
                global_params_path: da_params.global_params_path.clone(),
                verifier_sk: hex::encode(verifier_sk_bytes),
                verifier_index: subnetwork_ids,
                num_samples: da_params.num_samples,
                num_subnets: da_params.num_subnets,
                old_blobs_check_interval: da_params.old_blobs_check_interval,
                blobs_validity_duration: da_params.blobs_validity_duration,
            }
        })
        .collect()
}

fn build_da_peer_list(
    peer_ids: &[PeerId],
    listening_addresses: &[Multiaddr],
) -> HashMap<PeerId, Multiaddr> {
    peer_ids
        .iter()
        .zip(listening_addresses.iter())
        .map(|(peer_id, listening_address)| {
            let p2p_addr = listening_address.clone().with_p2p(*peer_id).unwrap();
            (*peer_id, p2p_addr)
        })
        .collect()
}

fn secret_key_to_peer_id(node_key: nomos_libp2p::ed25519::SecretKey) -> PeerId {
    PeerId::from_public_key(
        &nomos_libp2p::ed25519::Keypair::from(node_key)
            .public()
            .into(),
    )
}
