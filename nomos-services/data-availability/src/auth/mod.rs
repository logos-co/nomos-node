pub mod mock;

pub trait DaAuth {
    type Settings: Clone;

    fn new(settings: Self::Settings) -> Self;
}
