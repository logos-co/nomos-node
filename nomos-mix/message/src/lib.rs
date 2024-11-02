mod error;
mod layered_cipher;
pub mod packet;
mod routing;

pub use error::Error;

use packet::{Packet, UnpackedPacket};

pub struct MessageBuilder {
    max_layers: usize,
    max_payload_size: usize,
    drop_message: Vec<u8>,
}

#[repr(u8)]
pub enum MessageFlag {
    Drop = 0x00,
    Data = 0x01,
}

impl MessageBuilder {
    pub fn new(max_layers: usize, max_payload_size: usize) -> Result<Self, Error> {
        Ok(Self {
            max_layers,
            max_payload_size,
            drop_message: Self::new_drop_message(max_layers, max_payload_size)?,
        })
    }

    pub fn new_message(
        &self,
        recipient_pubkeys: Vec<[u8; 32]>,
        payload: &[u8],
    ) -> Result<Vec<u8>, Error> {
        let recipient_pubkeys = recipient_pubkeys
            .into_iter()
            .map(x25519_dalek::PublicKey::from)
            .collect::<Vec<_>>();
        let packet = Packet::build(
            &recipient_pubkeys,
            self.max_layers,
            payload,
            self.max_payload_size,
        )?;
        Ok(Self::concat_flag(MessageFlag::Data, &packet.to_bytes()))
    }

    fn new_drop_message(max_layers: usize, max_payload_size: usize) -> Result<Vec<u8>, Error> {
        let dummy_packet = Packet::build(
            &[x25519_dalek::PublicKey::from(
                &x25519_dalek::EphemeralSecret::random(),
            )],
            max_layers,
            &[0u8; 1],
            max_payload_size,
        )?;
        Ok(Self::concat_flag(
            MessageFlag::Drop,
            &dummy_packet.to_bytes(),
        ))
    }

    pub fn drop_message(&self) -> Vec<u8> {
        self.drop_message.clone()
    }

    pub fn is_drop_message(message: &[u8]) -> bool {
        Self::check_flag(MessageFlag::Drop, message).is_ok()
    }

    pub fn unpack_message(
        &self,
        message: &[u8],
        private_key: [u8; 32],
    ) -> Result<(Vec<u8>, bool), Error> {
        let message = Self::check_flag(MessageFlag::Data, message)?;
        let packet = Packet::from_bytes(message, self.max_layers)?;
        let private_key = x25519_dalek::StaticSecret::from(private_key);
        Ok(match packet.unpack(&private_key, self.max_layers)? {
            UnpackedPacket::ToForward(m) => {
                (Self::concat_flag(MessageFlag::Data, &m.to_bytes()), false)
            }
            UnpackedPacket::FullyUnpacked(m) => (m, true),
        })
    }

    pub fn message_size(&self) -> usize {
        self.drop_message.len()
    }

    fn concat_flag(flag: MessageFlag, bytes: &[u8]) -> Vec<u8> {
        concat_bytes(&[&[flag as u8], bytes])
    }

    fn check_flag(flag: MessageFlag, message: &[u8]) -> Result<&[u8], Error> {
        if message.first() != Some(&(flag as u8)) {
            return Err(Error::InvalidMixMessage);
        }
        if message.len() == 1 {
            Ok(&[])
        } else {
            Ok(&message[1..])
        }
    }
}

pub(crate) fn concat_bytes(bytes_list: &[&[u8]]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(bytes_list.iter().map(|bytes| bytes.len()).sum());
    bytes_list
        .iter()
        .for_each(|bytes| buf.extend_from_slice(bytes));
    buf
}

pub(crate) fn parse_bytes<'a>(data: &'a [u8], sizes: &[usize]) -> Result<Vec<&'a [u8]>, String> {
    let mut i = 0;
    sizes
        .iter()
        .map(|&size| {
            if i + size > data.len() {
                return Err("The sum of sizes exceeds the length of the input slice".to_string());
            }
            let slice = &data[i..i + size];
            i += size;
            Ok(slice)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_builder() {
        let builder = MessageBuilder::new(5, 2048).unwrap();

        let drop_message = builder.drop_message();
        assert_eq!(drop_message.len(), builder.message_size());

        let data_message = builder.new_message(vec![[1u8; 32]], &[10u8; 100]).unwrap();
        assert!(data_message != drop_message);
        assert_eq!(data_message.len(), builder.message_size());
    }
}
