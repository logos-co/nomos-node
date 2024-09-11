use overwatch_rs::{
    overwatch::{handle::OverwatchHandle, Services},
    services::{
        relay::{AnyMessage, OutboundRelay},
        ServiceData,
    },
};

pub struct ServiceWrapper<'a> {
    ow_handle: &'a OverwatchHandle,
    wrapped_services: Vec<&'a OutboundRelay<ServiceData::Message>>,
}

impl<'a> ServiceWrapper<'a> {
    pub fn new(handle: &'a OverwatchHandle) -> Self {
        Self {
            ow_handle: handle,
            wrapped_services: vec![],
        }
    }

    pub async fn wrap(&mut self, relay: &'a OutboundRelay<ServiceData::Message>) {
        self.wrapped_services.push(relay);
    }
}
