use crate::impl_from_for_message;

include!(concat!(env!("OUT_DIR"), "/nomos.da.v1.sampling.rs"));

impl_from_for_message!(
    Message,
    SampleReq => SampleReq,
    SampleRes => SampleRes,
);
