#[derive(Clone)]
pub struct AxumBackendSettings {
    pub addr: SocketAddr,
    pub handle: OverwatchHandle,
}

pub struct AxumBackend<T, S, const SIZE: usize> {
    settings: AxumBackendSettings,
    _tx: core::marker::PhantomData<T>,
    _storage_serde: core::marker::PhantomData<S>,
}
