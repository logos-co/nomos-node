use std::{fmt::Debug, io, pin::Pin};

use futures::AsyncWrite;

/// Trait for types that can be written to an `AsyncWrite` stream.
#[async_trait::async_trait]
pub trait AsyncWritable: Debug {
    async fn write(&self, writer: &mut Pin<Box<dyn AsyncWrite + Send>>) -> io::Result<()>;
}
