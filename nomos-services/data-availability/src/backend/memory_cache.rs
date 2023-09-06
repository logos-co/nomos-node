use moka::future::{Cache, CacheBuilder};
use nomos_core::blob::Blob;
use std::time::Duration;

pub struct BlobCacheSettings {
    max_capacity: usize,
    evicting_period: Duration,
}

pub struct BlobCache<H, B>(Cache<H, B>);

impl<B> BlobCache<B::Hash, B>
where
    B: Clone + Blob + Send + Sync + 'static,
    B::Hash: Send + Sync + 'static,
{
    pub fn new(settings: BlobCacheSettings) -> Self {
        let BlobCacheSettings {
            max_capacity,
            evicting_period,
        } = settings;
        let cache = CacheBuilder::new(max_capacity as u64)
            .time_to_live(evicting_period)
            // can we leverage this to evict really old blobs?
            .time_to_idle(evicting_period)
            .build();
        Self(cache)
    }

    pub async fn add(&self, blob: B) {
        self.0.insert(blob.hash(), blob).await
    }

    pub async fn remove(&self, hash: &B::Hash) {
        self.0.remove(hash).await;
    }

    pub async fn pending_blobs(&self) -> Box<dyn Iterator<Item = B> + Send + '_> {
        Box::new(self.0.iter().map(|t| t.1))
    }
}
