pub mod message_blend;
pub mod persistent_transmission;

pub enum MixOutgoingMessage {
    FullyUnwrapped(Vec<u8>),
    Outbound(Vec<u8>),
}
