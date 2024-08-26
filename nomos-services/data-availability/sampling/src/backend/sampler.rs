// std
use core::fmt;
// crates
use kzgrs_backend::common::blob::DaBlob;
use nomos_core::da::DaSampler;
// internal
use super::SamplingBackend;

#[derive(Debug)]
pub enum SamplingError {
    RequestedColumnNotFound,
    RequestedRowNotFound,
    RowColumnCombinationNotFound,
}

impl fmt::Display for SamplingError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            SamplingError::RequestedColumnNotFound => write!(f, "Requested column not found"),
            SamplingError::RequestedRowNotFound => write!(f, "Requested row not found"),
            SamplingError::RowColumnCombinationNotFound => {
                write!(f, "Requested row/column combination not found")
            }
        }
    }
}

impl std::error::Error for SamplingError {}

pub struct KzgrsDaSampler {
    settings: KzgrsDaSamplerSettings,
}

impl SamplingBackend for KzgrsDaSampler {
    type Settings = KzgrsDaSamplerSettings;

    fn new(settings: Self::Settings) -> Self {
        Self { settings: settings }
    }
}

impl DaSampler for KzgrsDaSampler {
    type DaBlob = DaBlob;
    type DaRow = u16;
    type DaCol = u16;

    fn sample(&self, blob: &Self::DaBlob, col: &Self::DaCol, row: &Self::DaRow) {
        for i in self.settings.num_samples {}
    }
}

#[derive(Debug, Clone)]
pub struct KzgrsDaSamplerSettings {
    pub num_samples: u16,
}
