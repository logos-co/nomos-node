pub mod coin_flipping;

pub trait Queue<T> {
    fn push(&mut self, data: T);
    fn pop(&mut self) -> T;
}
