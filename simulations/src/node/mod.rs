pub type NodeId = usize;

pub trait Node {
    type Settings;
    fn new(settings: Self::Settings) -> Self;
    fn id(&self) -> NodeId;
    fn run(&mut self);
}
