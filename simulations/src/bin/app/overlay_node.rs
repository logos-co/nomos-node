use consensus_engine::overlay::{BranchOverlay, RandomBeaconState};
use consensus_engine::{
    overlay::{FlatOverlay, FreezeMembership, RoundRobin, TreeOverlay},
    NodeId,
};
use rand::Rng;
use simulations::{
    network::InMemoryNetworkInterface,
    node::carnot::{messages::CarnotMessage, CarnotNode, CarnotSettings, CarnotState},
    runner::BoxedNode,
    settings::SimulationSettings,
};

pub fn to_overlay_node<R: Rng>(
    node_id: NodeId,
    nodes: Vec<NodeId>,
    leader: NodeId,
    network_interface: InMemoryNetworkInterface<CarnotMessage>,
    genesis: nomos_core::block::Block<[u8; 32]>,
    mut rng: R,
    settings: &SimulationSettings,
) -> BoxedNode<CarnotSettings, CarnotState> {
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
            };
            Box::new(
                CarnotNode::<TreeOverlay<RoundRobin, RandomBeaconState>>::new(
                    node_id,
                    CarnotSettings::new(
                        settings.node_settings.timeout,
                        settings.record_settings.clone(),
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
                number_of_levels: branch_settings.number_of_levels,
                leader: RoundRobin::new(),
                committee_membership: RandomBeaconState::initial_sad_from_entropy([0; 32]),
            };
            Box::new(
                CarnotNode::<BranchOverlay<RoundRobin, RandomBeaconState>>::new(
                    node_id,
                    CarnotSettings::new(
                        settings.node_settings.timeout,
                        settings.record_settings.clone(),
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
