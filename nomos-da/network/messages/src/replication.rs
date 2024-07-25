use crate::{common, impl_from_for_message};

include!(concat!(env!("OUT_DIR"), "/nomos.da.v1.replication.rs"));

impl_from_for_message!(
    Message,
    ReplicationReq => ReplicationReq,
    common::SessionReq => SessionReq,
);
