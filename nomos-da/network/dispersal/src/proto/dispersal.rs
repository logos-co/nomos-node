use crate::{impl_from_for_dispersal_message, proto::common};

include!(concat!(env!("OUT_DIR"), "/nomos.da.v1.dispersal.rs"));

impl_from_for_dispersal_message!(
    DispersalReq => DispersalReq,
    DispersalRes => DispersalRes,
    common::SessionReq => SessionReq,
);

#[cfg(test)]
mod tests {
    use crate::proto::{common, dispersal};

    #[test]
    fn dispersal_message() {
        let blob = common::Blob {
            blob_id: vec![0; 32],
            data: vec![1; 32],
        };
        let req = dispersal::DispersalReq { blob: Some(blob) };

        assert_eq!(req.blob.unwrap().blob_id, vec![0; 32]);
    }
}
