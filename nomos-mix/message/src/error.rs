#[derive(Debug)]
pub enum Error {
    /// Invalid mix message format
    InvalidMixMessage,
    /// Payload size is too large
    PayloadTooLarge,
    /// Unwrapping a message is not allowed
    /// (e.g. the message cannot be unwrapped using the private key provided)
    MsgUnwrapNotAllowed,
}
