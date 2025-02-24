use services_utils::overwatch::recovery::backends::FileBackendSettings;

#[derive(Clone, Debug, Default)]
pub struct TxMempoolSettings<PoolSettings, NetworkAdapterSettings> {
    pub pool: PoolSettings,
    pub network_adapter: NetworkAdapterSettings,
}

impl<PoolSettings, NetworkAdapterSettings> FileBackendSettings
    for TxMempoolSettings<PoolSettings, NetworkAdapterSettings>
where
    PoolSettings: FileBackendSettings,
{
    fn recovery_file(&self) -> &std::path::PathBuf {
        self.pool.recovery_file()
    }
}
