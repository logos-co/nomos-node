use crate::{common, impl_from_for_message};

include!(concat!(env!("OUT_DIR"), "/nomos.da.v1.broadcast.rs"));

impl_from_for_message!(
    Message,
    BroadcastReq => BroadcastReq,
    common::SessionReq => SessionReq,
);
