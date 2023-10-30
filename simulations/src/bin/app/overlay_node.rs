use consensus_engine::overlay::{BranchOverlay, FisherYatesShuffle, RandomBeaconState};
use consensus_engine::Overlay;
use consensus_engine::{
    overlay::{FlatOverlay, FreezeMembership, RoundRobin, TreeOverlay},
    NodeId,
};
use rand::Rng;
use simulations::overlay::overlay_info::{OverlayInfo, OverlayInfoExt};
use simulations::settings::OverlaySettings;
use simulations::{
    network::InMemoryNetworkInterface,
    node::carnot::{messages::CarnotMessage, CarnotNode, CarnotSettings, CarnotState},
    runner::BoxedNode,
    settings::SimulationSettings,
};

pub fn overlay_info(
    nodes: Vec<NodeId>,
    leader: NodeId,
    overlay_settings: &OverlaySettings,
) -> OverlayInfo {
    match &overlay_settings {
        simulations::settings::OverlaySettings::Flat => {
            FlatOverlay::<RoundRobin, FisherYatesShuffle>::new(
                consensus_engine::overlay::FlatOverlaySettings {
                    nodes: nodes.to_vec(),
                    leader: RoundRobin::new(),
                    leader_super_majority_threshold: None,
                },
            )
            .info()
        }
        simulations::settings::OverlaySettings::Tree(tree_settings) => {
            TreeOverlay::new(consensus_engine::overlay::TreeOverlaySettings {
                nodes,
                current_leader: leader,
                number_of_committees: tree_settings.number_of_committees,
                leader: RoundRobin::new(),
                committee_membership: RandomBeaconState::initial_sad_from_entropy([0; 32]),
                super_majority_threshold: None,
            })
            .info()
        }
        simulations::settings::OverlaySettings::Branch(branch_settings) => {
            BranchOverlay::new(consensus_engine::overlay::BranchOverlaySettings {
                nodes,
                current_leader: leader,
                branch_depth: branch_settings.branch_depth,
                leader: RoundRobin::new(),
                committee_membership: RandomBeaconState::initial_sad_from_entropy([0; 32]),
            })
            .info()
        }
    }
}

pub fn to_overlay_node<R: Rng>(
    node_id: NodeId,
    nodes: Vec<NodeId>,
    leader: NodeId,
    network_interface: InMemoryNetworkInterface<CarnotMessage>,
    genesis: nomos_core::block::Block<[u8; 32], Box<[u8]>>,
    mut rng: R,
    settings: &SimulationSettings,
) -> BoxedNode<CarnotSettings, CarnotState> {
    let fmt = match &settings.stream_settings {
        simulations::streaming::StreamSettings::Naive(n) => n.format,
        simulations::streaming::StreamSettings::IO(_) => {
            simulations::streaming::SubscriberFormat::Csv
        }
        #[cfg(feature = "polars")]
        simulations::streaming::StreamSettings::Polars(p) => p.format,
    };
    match &settings.overlay_settings {
        simulations::settings::OverlaySettings::Flat => {
            let overlay_settings = consensus_engine::overlay::FlatOverlaySettings {
                nodes: nodes.to_vec(),
                leader: RoundRobin::new(),
                leader_super_majority_threshold: None,
            };
            Box::new(
                CarnotNode::<FlatOverlay<RoundRobin, FreezeMembership>>::new(
                    node_id,
                    CarnotSettings::new(
                        settings.node_settings.timeout,
                        settings.record_settings.clone(),
                        fmt,
                    ),
                    overlay_settings,
                    genesis,
                    network_interface,
                    &mut rng,
                ),
            )
        }
        simulations::settings::OverlaySettings::Tree(tree_settings) => {
            let overlay_settings = consensus_engine::overlay::TreeOverlaySettings {
                nodes,
                current_leader: leader,
                number_of_committees: tree_settings.number_of_committees,
                leader: RoundRobin::new(),
                committee_membership: RandomBeaconState::initial_sad_from_entropy([0; 32]),
                super_majority_threshold: None,
            };
            Box::new(
                CarnotNode::<TreeOverlay<RoundRobin, RandomBeaconState>>::new(
                    node_id,
                    CarnotSettings::new(
                        settings.node_settings.timeout,
                        settings.record_settings.clone(),
                        fmt,
                    ),
                    overlay_settings,
                    genesis,
                    network_interface,
                    &mut rng,
                ),
            )
        }
        simulations::settings::OverlaySettings::Branch(branch_settings) => {
            let overlay_settings = consensus_engine::overlay::BranchOverlaySettings {
                nodes,
                current_leader: leader,
                branch_depth: branch_settings.branch_depth,
                leader: RoundRobin::new(),
                committee_membership: RandomBeaconState::initial_sad_from_entropy([0; 32]),
            };
            Box::new(
                CarnotNode::<BranchOverlay<RoundRobin, RandomBeaconState>>::new(
                    node_id,
                    CarnotSettings::new(
                        settings.node_settings.timeout,
                        settings.record_settings.clone(),
                        fmt,
                    ),
                    overlay_settings,
                    genesis,
                    network_interface,
                    &mut rng,
                ),
            )
        }
    }
}
