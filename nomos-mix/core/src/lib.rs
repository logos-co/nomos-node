pub mod cover_traffic;
pub mod membership;
pub mod message_blend;
pub mod persistent_transmission;

pub enum MixOutgoingMessage {
    FullyUnwrapped(Vec<u8>),
    Outbound(Vec<u8>),
}

impl From<MixOutgoingMessage> for Vec<u8> {
    fn from(value: MixOutgoingMessage) -> Self {
        match value {
            MixOutgoingMessage::FullyUnwrapped(v) => v,
            MixOutgoingMessage::Outbound(v) => v,
        }
    }
}
