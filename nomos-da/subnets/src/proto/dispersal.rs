include!(concat!(env!("OUT_DIR"), "/nomos.da.dispersal.v1.rs"));

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
