// std
use std::collections::HashMap;
use std::fmt::Debug;
// crates
// internal
use crate::{MetricsBackend, OwnedServiceId};
use overwatch_rs::services::ServiceId;

#[derive(Debug, Clone)]
pub struct MapMetricsBackend<MetricsData>(HashMap<ServiceId, MetricsData>);

#[async_trait::async_trait]
impl<MetricsData: Clone + Debug + Send + Sync + 'static> MetricsBackend
    for MapMetricsBackend<MetricsData>
{
    type MetricsData = MetricsData;
    type Error = ();
    type Settings = ();

    fn init(_config: Self::Settings) -> Self {
        Self(HashMap::new())
    }

    async fn update(&mut self, service_id: ServiceId, data: Self::MetricsData) {
        self.0.insert(service_id, data);
    }

    async fn load(&self, service_id: &OwnedServiceId) -> Option<&Self::MetricsData> {
        self.0.get(service_id.as_ref())
    }
}
