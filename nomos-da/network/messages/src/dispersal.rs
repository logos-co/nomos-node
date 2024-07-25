use crate::{common, impl_from_for_message};

include!(concat!(env!("OUT_DIR"), "/nomos.da.v1.dispersal.rs"));

impl_from_for_message!(
    Message,
    DispersalReq => DispersalReq,
    DispersalRes => DispersalRes,
    common::SessionReq => SessionReq,
);

#[cfg(test)]
mod tests {
    use crate::{common, dispersal};

    #[test]
    fn dispersal_message() {
        let blob = common::Blob {
            blob_id: vec![0; 32],
            data: vec![1; 32],
        };
        let req = dispersal::DispersalReq {
            blob: Some(blob),
            subnetwork_id: 0,
        };

        assert_eq!(req.blob.unwrap().blob_id, vec![0; 32]);
    }
}
