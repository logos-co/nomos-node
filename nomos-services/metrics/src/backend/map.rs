// std
use std::collections::HashMap;
use std::fmt::Debug;
// crates
// internal
use crate::MetricsBackend;
use overwatch_rs::services::ServiceId;

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

    async fn load(&mut self, service_id: ServiceId) -> Option<&Self::MetricsData> {
        self.0.get(&service_id)
    }
}
