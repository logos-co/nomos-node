#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Not sufficient nodes")]
    NotSufficientNodes,
    #[error("Mix message error: {0}")]
    MixMessageError(#[from] nomos_mix_message::Error),
}
