#[cfg(feature = "mock")]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum MockTransactionMsg {
    Request(nomos_network::backends::mock::MockMessage),
    Response(nomos_network::backends::mock::MockMessage),
}

#[cfg(feature = "mock")]
impl From<&MockTransactionMsg> for String {
    fn from(msg: &MockTransactionMsg) -> Self {
        match msg {
            MockTransactionMsg::Request(msg) | MockTransactionMsg::Response(msg) => msg.payload(),
        }
    }
}
