use std::path::PathBuf;

use services_utils::overwatch::recovery::backends::FileBackendSettings;

#[derive(Clone, Debug, Default)]
pub struct TxMempoolSettings<PoolSettings, NetworkAdapterSettings> {
    pub pool: PoolSettings,
    pub network_adapter: NetworkAdapterSettings,
    pub recovery_path: PathBuf,
}

impl<PoolSettings, NetworkAdapterSettings> FileBackendSettings
    for TxMempoolSettings<PoolSettings, NetworkAdapterSettings>
{
    fn recovery_file(&self) -> &std::path::PathBuf {
        &self.recovery_path
    }
}
