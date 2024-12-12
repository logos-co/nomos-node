mod cluster;
pub mod config;
mod node;
pub mod test_case;

pub trait TestCase {
    fn name(&self) -> &'static str;
    fn run(&self) -> Result<(), anyhow::Error>;
}
