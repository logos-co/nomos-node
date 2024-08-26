pub mod sampler;

use nomos_core::da::DaSampler;

pub trait SamplingBackend: DaSampler {
    type Settings;
    fn new(settings: Self::Settings) -> Self;
}
