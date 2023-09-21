use fraction::{Fraction, GenericFraction, ToPrimitive};

const SUPER_MAJORITY_THRESHOLD_NUM: u64 = 2;
const SUPER_MAJORITY_THRESHOLD_DEN: u64 = 3;

pub(crate) fn default_super_majority_threshold() -> GenericFraction<u64> {
    Fraction::new(SUPER_MAJORITY_THRESHOLD_NUM, SUPER_MAJORITY_THRESHOLD_DEN)
}

pub(crate) fn apply_threshold(size: usize, threshold: GenericFraction<u64>) -> usize {
    // `threshold` is a tuple of (num, den) where `num/den` is the super majority threshold
    (Fraction::from(size) * threshold)
        .ceil()
        .to_usize()
        .unwrap()
}

pub mod deser_fraction {
    use fraction::Fraction;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
    use std::str::FromStr;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Fraction>, D::Error>
    where
        D: Deserializer<'de>,
    {
        <Option<String>>::deserialize(deserializer)?
            .map(|s| FromStr::from_str(&s).map_err(de::Error::custom))
            .transpose()
    }

    pub fn serialize<S>(value: &Option<Fraction>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.map(|v| v.to_string()).serialize(serializer)
    }
}
