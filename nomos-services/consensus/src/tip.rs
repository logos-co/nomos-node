use nomos_core::block::BlockId;

/// Assuming determining which tip to consider is integral part of consensus
#[derive(Clone)]
pub struct Tip;

impl Tip {
    pub fn id(&self) -> BlockId {
        unimplemented!()
    }
}
