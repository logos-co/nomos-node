use std::error::Error;

pub trait StorageBackend {
    type Config: Clone + Send + Sync + 'static;
    type Error: Error;
    type Transaction;
    fn new(config: Self::Config) -> Self;
    fn store<K: AsRef<[u8]>, T: Into<Vec<u8>>>(
        &mut self,
        key: K,
        value: T,
    ) -> Result<(), Self::Error>;
    fn load<K: AsRef<[u8]>, T: From<Vec<u8>> + Sized>(&mut self, key: &K)
        -> Result<T, Self::Error>;
    fn remove<K: AsRef<[u8]>, T: From<Vec<u8>>>(
        &mut self,
        key: &K,
    ) -> Result<Option<T>, Self::Error>;
    fn execute(&mut self, transaction: Self::Transaction) -> Result<(), Self::Error>;
}
