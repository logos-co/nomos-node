pub mod disseminate;
pub mod network;
// pub mod retrieve;

use network::backend::ExecutorBackend;
use subnetworks_assignations::versions::v1::FillFromNodeList;

pub type NetworkBackend = ExecutorBackend<FillFromNodeList>;
