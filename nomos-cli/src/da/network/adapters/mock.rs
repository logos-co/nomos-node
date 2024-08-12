use kzgrs_backend::encoder::EncodedData as KzgEncodedData;
use nomos_core::da::DaDispersal;
use nomos_da_network_service::{backends::mock::executor::MockExecutorBackend, NetworkService};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};
type Relay<T> = OutboundRelay<<NetworkService<T> as ServiceData>::Message>;

pub struct MockExecutorDispersalAdapter {
    network_relay: Relay<MockExecutorBackend>,
}

impl MockExecutorDispersalAdapter {
    pub fn new(network_relay: Relay<MockExecutorBackend>) -> Self {
        Self { network_relay }
    }
}

impl DaDispersal for MockExecutorDispersalAdapter {
    type EncodedData = KzgEncodedData;
    type Error = DynError;

    fn disperse(&self, encoded_data: Self::EncodedData) -> Result<(), Self::Error> {
        todo!()
    }
}
