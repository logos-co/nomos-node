// std
use std::fmt::Debug;
use std::pin::Pin;

// crates
use futures::{Stream, StreamExt};
use libp2p_identity::PeerId;
use tokio::sync::oneshot;
// internal
use crate::network::adapters::common::adapter_for;
use crate::network::NetworkAdapter;
use nomos_core::da::BlobId;
use nomos_da_network_core::SubnetworkId;
use nomos_da_network_service::backends::libp2p::{
    common::SamplingEvent,
    validator::{DaNetworkEvent, DaNetworkEventKind, DaNetworkMessage, DaNetworkValidatorBackend},
};
use nomos_da_network_service::{DaNetworkMsg, NetworkService};
use overwatch_rs::services::relay::OutboundRelay;
use overwatch_rs::services::ServiceData;
use overwatch_rs::DynError;
use subnetworks_assignations::MembershipHandler;

adapter_for!(
    DaNetworkValidatorBackend,
    DaNetworkMessage,
    DaNetworkEventKind,
    DaNetworkEvent
);
