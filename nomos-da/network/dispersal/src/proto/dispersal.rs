include!(concat!(env!("OUT_DIR"), "/nomos.da.dispersal.v1.rs"));

// Macro to implement From trait for DispersalMessage
macro_rules! impl_from_for_dispersal_message {
    ($($type:ty => $variant:ident),+ $(,)?) => {
        $(
            impl From<$type> for DispersalMessage {
                fn from(msg: $type) -> Self {
                    DispersalMessage {
                        message_type: Some(dispersal_message::MessageType::$variant(msg)),
                    }
                }
            }
        )+
    }
}

impl_from_for_dispersal_message!(
    DispersalReq => DispersalReq,
    DispersalRes => DispersalRes,
    SampleReq => SampleReq,
    SampleRes => SampleRes,
    SessionReq => SessionReq,
);

#[cfg(test)]
mod tests {
    use crate::proto::dispersal;

    #[test]
    fn dispersal_message() {
        let blob = dispersal::Blob {
            blob_id: vec![0; 32],
            data: vec![1; 32],
        };
        let req = dispersal::DispersalReq { blob: Some(blob) };

        assert_eq!(req.blob.unwrap().blob_id, vec![0; 32]);
    }
}
