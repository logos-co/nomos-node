// std
// crates
use color_eyre::eyre::Result;
use nomos_da_network_service::backends::libp2p::executor::DaNetworkExecutorBackend;
use nomos_da_network_service::NetworkService as DaNetworkService;
use nomos_network::backends::libp2p::Libp2p as NetworkBackend;
use nomos_node::{
    config::{update_cryptarchia_consensus, update_log, update_network},
    CryptarchiaArgs, HttpArgs, LogArgs, Logger, NetworkArgs, NetworkService, Wire,
};
use nomos_storage::backends::rocksdb::RocksBackend;
use overwatch_rs::services::ServiceData;
use serde::{Deserialize, Serialize};
use subnetworks_assignations::versions::v1::FillFromNodeList;
// internal
use crate::ExecutorApiService;

#[derive(Deserialize, Debug, Clone, Serialize)]
pub struct Config {
    pub log: <Logger as ServiceData>::Settings,
    pub network: <NetworkService<NetworkBackend> as ServiceData>::Settings,
    pub da_dispersal: <crate::DaDispersal as ServiceData>::Settings,
    pub da_network:
        <DaNetworkService<DaNetworkExecutorBackend<FillFromNodeList>> as ServiceData>::Settings,
    pub da_indexer: <crate::DaIndexer as ServiceData>::Settings,
    pub da_verifier: <crate::DaVerifier as ServiceData>::Settings,
    pub da_sampling: <crate::DaSampling as ServiceData>::Settings,
    pub http: <ExecutorApiService as ServiceData>::Settings,
    pub cryptarchia: <crate::Cryptarchia as ServiceData>::Settings,
    pub storage: <crate::StorageService<RocksBackend<Wire>> as ServiceData>::Settings,
    pub wait_online_secs: u64,
}

impl Config {
    pub fn update_from_args(
        mut self,
        log_args: LogArgs,
        network_args: NetworkArgs,
        http_args: HttpArgs,
        cryptarchia_args: CryptarchiaArgs,
    ) -> Result<Self> {
        update_log(&mut self.log, log_args)?;
        update_network(&mut self.network, network_args)?;
        update_http(&mut self.http, http_args)?;
        update_cryptarchia_consensus(&mut self.cryptarchia, cryptarchia_args)?;
        Ok(self)
    }
}

pub fn update_http(
    http: &mut <ExecutorApiService as ServiceData>::Settings,
    http_args: HttpArgs,
) -> Result<()> {
    let HttpArgs {
        http_addr,
        cors_origins,
    } = http_args;

    if let Some(addr) = http_addr {
        http.backend_settings.address = addr;
    }

    if let Some(cors) = cors_origins {
        http.backend_settings.cors_origins = cors;
    }

    Ok(())
}
