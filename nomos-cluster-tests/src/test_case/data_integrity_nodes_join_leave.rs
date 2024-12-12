use crate::TestCase;
use anyhow::Error;

pub struct DataIntegrityNodesJoinLeave;

impl TestCase for DataIntegrityNodesJoinLeave {
    fn name(&self) -> &'static str {
        module_path!().split("::").last().unwrap()
    }

    fn run(&self) -> Result<(), Error> {
        todo!()
    }
}
