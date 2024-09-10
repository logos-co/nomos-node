use overwatch_rs::{
    overwatch::{handle::OverwatchHandle, OverwatchRunner, Services},
    services::ServiceData,
};

pub struct ServiceWrapper<S> {
    ow_handle: OverwatchHandle,
    wrapped_services: Vec<S>,
}

impl<S: Services> ServiceWrapper<S>
where
    S: Services + Send,
{
    fn new(settings: S::Settings, relays: Vec<ServiceData>) -> Self {
        let ow = OverwatchRunner::<S>::run(settings, None)
            .map_err(|e| eprintln!("Error encountered: {}", e))
            .unwrap();
        let handle = ow.handle();
        let mut services = vec![];
        for s in relays.iter() {
            let relay = handle.relay::<s>();
            services.push(relay);
        }
        Self {
            ow_handle: handle,
            wrapped_services: services,
        }
    }
}
