#[derive(Debug)]
pub enum Error {
    /// Invalid mix message format
    InvalidMixMessage,
    /// Payload size is too large
    PayloadTooLarge,
    /// Unwrapping a message is not allowed
    MsgUnwrapNotAllowed,
}
