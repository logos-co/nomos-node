// // std
// use std::collections::BTreeSet;
// use std::fmt::Debug;
// // crates
// //
// // internal
// use super::DaSamplingServiceBackend;
// use super::NetworkAdapter;
// use nomos_core::da::blob::Blob;
// use nomos_core::da::BlobId;
// use nomos_da_network_service::backends::NetworkBackend;
// use overwatch_rs::services::state::ServiceState;
//
// pub struct KzgrsDaSampler<Backend, S>
// where
//     Backend: NetworkBackend + 'static,
//     Backend::Settings: Clone + Debug + Send + Sync + 'static,
//     Backend::State: ServiceState<Settings = Backend::Settings> + Clone + Send + Sync,
//     Backend::Message: Debug + Send + Sync + 'static,
//     Backend::EventKind: Debug + Send + Sync + 'static,
//     Backend::NetworkEvent: Debug + Send + Sync + 'static,
//     S: Clone,
// {
//     settings: KzgrsDaSamplerSettings,
//     validated_blobs: BTreeSet<BlobId>,
//     network_adapter: NetworkAdapter<Backend = Backend, Settings = Clone>,
// }
//
// impl<Backend, S> DaSamplingServiceBackend<Backend, S> for KzgrsDaSampler<Backend, S>
// where
//     Backend: NetworkBackend + 'static,
//     Backend::Settings: Clone + Debug + Send + Sync + 'static,
//     Backend::State: ServiceState<Settings = Backend::Settings> + Clone + Send + Sync,
//     Backend::Message: Debug + Send + Sync + 'static,
//     Backend::EventKind: Debug + Send + Sync + 'static,
//     Backend::NetworkEvent: Debug + Send + Sync + 'static,
//     S: Clone,
// {
//     type Settings = KzgrsDaSamplerSettings;
//     type BlobId = BlobId;
//     type Blob = Blob;
//
//     fn new(
//         settings: Self::Settings,
//         network_adapter: NetworkAdapter<Backend = Backend, Settings = Clone>,
//     ) -> Self {
//         let bt: BTreeSet<BlobId> = BTreeSet::new();
//         Self {
//             settings: settings,
//             validated_blobs: bt,
//             network_adapter: network_adapter,
//         }
//     }
//
//     fn trigger_sampling(&self, blob_id: BlobId) {
//         self.network_adapter.start_sampling(blob_id);
//     }
//
//     fn get_validated_blobs(&self) -> BTreeSet<BlobId> {
//         self.validated_blobs.clone()
//     }
//
//     fn mark_blob_validated(&self, blob_id: BlobId) {
//         self.validated_blobs.insert(blob_id);
//     }
// }
//
// #[derive(Debug, Clone)]
// pub struct KzgrsDaSamplerSettings {
//     pub num_samples: u16,
// }
