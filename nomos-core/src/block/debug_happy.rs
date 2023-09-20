use consensus_engine::{AggregateQc, Qc, View};
use parking_lot::Mutex;
use std::{fs::File, hash::Hash, sync::OnceLock};

static TEMP_DEBUG_FILE: OnceLock<Mutex<File>> = OnceLock::new();

pub fn debug_file() -> &'static Mutex<File> {
    // Unwrap because we will only use this function in tests, panic anyway.
    TEMP_DEBUG_FILE.get_or_init(|| Mutex::new(tempfile::tempfile().unwrap()))
}

impl<Tx: Clone + Eq + Hash, Blob: Clone + Eq + Hash> Drop for super::Block<Tx, Blob> {
    fn drop(&mut self) {
        let header = self.header();
        // We need to persist the block to disk before dropping it
        // if there is a aggregated qc when testing happy path
        if let Qc::Aggregated(qcs) = &header.parent_qc {
            #[derive(serde::Serialize)]
            struct Info<'a> {
                id: String,
                view: View,
                qcs: &'a AggregateQc,
            }

            let mut file = debug_file().lock();
            let info = Info {
                id: format!("{}", header.id),
                view: header.view,
                qcs,
            };
            // Use pretty print to make it easier to read, because we need the this for debugging.
            serde_json::to_writer_pretty(&mut *file, &info).unwrap();
        }
    }
}
