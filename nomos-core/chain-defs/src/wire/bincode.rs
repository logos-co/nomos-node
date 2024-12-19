// Crates
use bincode::config::{
    Bounded, FixintEncoding, LittleEndian, RejectTrailing, WithOtherEndian, WithOtherIntEncoding,
    WithOtherLimit, WithOtherTrailing,
};
use bincode::de::read::SliceReader;
use bincode::Options;
use once_cell::sync::Lazy;

// Type composition is cool but also makes naming types a bit awkward
pub(crate) type BincodeOptions = WithOtherTrailing<
    WithOtherIntEncoding<
        WithOtherLimit<WithOtherEndian<bincode::DefaultOptions, LittleEndian>, Bounded>,
        FixintEncoding,
    >,
    RejectTrailing,
>;

// TODO: Remove this once we transition to smaller proofs
// Risc0 proofs are HUGE (220 Kb) and it's the only reason we need to have this limit so large
pub(crate) const DATA_LIMIT: u64 = 1 << 18; // Do not serialize/deserialize more than 256 KiB
pub(crate) static OPTIONS: Lazy<BincodeOptions> = Lazy::new(|| {
    bincode::DefaultOptions::new()
        .with_little_endian()
        .with_limit(DATA_LIMIT)
        .with_fixint_encoding()
        .reject_trailing_bytes()
});

pub(crate) type BincodeDeserializer<'de> = bincode::Deserializer<SliceReader<'de>, BincodeOptions>;
pub(crate) type BincodeSerializer<T> = bincode::Serializer<T, BincodeOptions>;
