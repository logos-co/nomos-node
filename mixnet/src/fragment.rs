use crate::error::MixnetError;

#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) struct Fragment {}

impl Fragment {
    pub(crate) fn from_bytes(value: &[u8]) -> Result<Self, MixnetError> {
        todo!()
    }
}

pub struct MessageReconstructor {}

impl MessageReconstructor {
    pub fn new() -> Self {
        todo!()
    }

    pub fn add(&mut self, fragment: Fragment) -> Option<Vec<u8>> {
        todo!()
    }
}
