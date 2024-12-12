use anyhow::Error;
use crate::TestCase;

pub struct DataIntegrityNodesJoinLeave;

impl TestCase for DataIntegrityNodesJoinLeave {
    fn name(&self) -> &'static str {
        module_path!().split("::").last().unwrap()
    }

    fn run(&self) -> Result<(), Error> {
        todo!()
    }
}