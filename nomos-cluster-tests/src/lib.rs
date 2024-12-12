mod cluster;
mod node;
pub mod config;
pub mod test_case;

pub trait TestCase {
    fn name(&self) -> &'static str;
    fn run(&self) -> Result<(), anyhow::Error>;
}

