use crate::node::NomosNode;

pub trait Cluster {
    fn members(&self) -> Vec<Box<dyn NomosNode>>;
}
