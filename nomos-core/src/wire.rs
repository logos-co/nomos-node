//! Serializer and Deserializer for wire formats.

// TODO: we're using bincode for now, but might need strong guarantees about
// the underlying format in the future for standardization.
use bincode::{
    config::{
        Bounded, DefaultOptions, FixintEncoding, LittleEndian, RejectTrailing, WithOtherEndian,
        WithOtherIntEncoding, WithOtherLimit, WithOtherTrailing,
    },
    de::read::SliceReader,
    Options,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

pub type Error = bincode::Error;
// type composition is cool but also makes naming types a bit akward
type BincodeOptions = WithOtherTrailing<
    WithOtherIntEncoding<
        WithOtherLimit<WithOtherEndian<DefaultOptions, LittleEndian>, Bounded>,
        FixintEncoding,
    >,
    RejectTrailing,
>;

const DATA_LIMIT: u64 = 2048; // Do not serialize/deserialize more than 2Kb
static OPTIONS: Lazy<BincodeOptions> = Lazy::new(|| {
    bincode::DefaultOptions::new()
        .with_little_endian()
        .with_limit(DATA_LIMIT)
        .with_fixint_encoding()
        .reject_trailing_bytes()
});

type BincodeDeserializer<'de> = bincode::Deserializer<SliceReader<'de>, BincodeOptions>;
type BincodeSerializer<T> = bincode::Serializer<T, BincodeOptions>;

pub struct Deserializer<'de> {
    inner: BincodeDeserializer<'de>,
}

pub struct Serializer<T> {
    inner: BincodeSerializer<T>,
}

impl<'de> Deserializer<'de> {
    pub fn get_deserializer(&mut self) -> impl serde::Deserializer<'de> + '_ {
        &mut self.inner
    }

    pub fn deserialize<T: Deserialize<'de>>(&mut self) -> Result<T, Error> {
        <T>::deserialize(&mut self.inner)
    }
}

impl<T: std::io::Write> Serializer<T> {
    pub fn get_serializer(&mut self) -> impl serde::Serializer + '_ {
        &mut self.inner
    }

    pub fn serialize_into<U: Serialize>(
        &mut self,
        item: &U,
    ) -> Result<<&mut BincodeSerializer<T> as serde::Serializer>::Ok, Error> {
        item.serialize(&mut self.inner)
    }
}

/// Return a deserializer for wire format
///
/// We only operator on in-memory slices as to abstract
/// any underlying protocol. See https://sans-io.readthedocs.io/how-to-sans-io.html
pub fn deserializer(data: &[u8]) -> Deserializer<'_> {
    Deserializer {
        inner: bincode::de::Deserializer::from_slice(data, *OPTIONS),
    }
}

/// Return a serializer for wire format
///
/// We only operator on in-memory slices as to abstract
/// any underlying protocol. See https://sans-io.readthedocs.io/how-to-sans-io.html
pub fn serializer(buffer: &mut Vec<u8>) -> Serializer<&'_ mut Vec<u8>> {
    Serializer {
        inner: bincode::Serializer::new(buffer, *OPTIONS),
    }
}

/// Return a serializer for wire format
///
/// We only operator on in-memory slices as to abstract
/// any underlying protocol. See https://sans-io.readthedocs.io/how-to-sans-io.html
pub fn serializer_into_slice(buffer: &mut [u8]) -> Serializer<&'_ mut [u8]> {
    Serializer {
        inner: bincode::Serializer::new(buffer, *OPTIONS),
    }
}

/// Serialize an object directly into a vec
pub fn serialize<T: Serialize>(item: &T) -> Result<Vec<u8>, Error> {
    let size = OPTIONS.serialized_size(item)?;
    let mut buf = Vec::with_capacity(size as usize);
    serializer(&mut buf).serialize_into(item)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ser_de() {
        let tmp = String::from("much wow, very cool");
        let mut buf = Vec::new();
        let _ = serializer(&mut buf).serialize_into(&tmp).unwrap();
        let deserialized = deserializer(&buf).deserialize::<String>().unwrap();
        assert_eq!(tmp, deserialized);
    }

    #[test]
    fn ser_de_slice() {
        let tmp = String::from("much wow, very cool");
        let mut buf = vec![0; 1024];
        let _ = serializer_into_slice(&mut buf)
            .serialize_into(&tmp)
            .unwrap();
        let deserialized = deserializer(&buf).deserialize::<String>().unwrap();
        assert_eq!(tmp, deserialized);
    }

    #[test]
    fn ser_de_owned() {
        let tmp = String::from("much wow, very cool");
        let serialized = serialize(&tmp).unwrap();
        let deserialized = deserializer(&serialized).deserialize::<String>().unwrap();
        assert_eq!(tmp, deserialized);
    }

    #[test]
    fn ser_de_inner() {
        let tmp = String::from("much wow, very cool");
        let mut buf = Vec::new();
        let mut serializer = serializer(&mut buf);
        tmp.serialize(serializer.get_serializer()).unwrap();
        let mut deserializer = deserializer(&buf);
        let deserialized = <String>::deserialize(deserializer.get_deserializer()).unwrap();
        assert_eq!(tmp, deserialized);
    }
}
