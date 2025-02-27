use serde::de::Error;
use serde::{Deserialize, Deserializer};
use std::fmt::Display;
use std::marker::PhantomData;
use time::Duration;

// TODO: This type can be fancier once const for generic types is implemented
// https://doc.rust-lang.org/beta/unstable-book/language-features/adt-const-params.html
// This means that we could add different bound from a const configuration checked on compile time.
// This type
struct MinimalBoundedDuration<const MIN_DURATION: usize, TAG: TimeTag> {
    duration: Duration,
    _tag: PhantomData<*const TAG>,
}

pub(crate) trait TimeTag {
    fn inner() -> char;
}

pub struct BoundTag<const TAG: char> {}
impl<const TAG: char> TimeTag for BoundTag<TAG> {
    fn inner() -> char {
        TAG
    }
}

pub type NANO = BoundTag<'n'>;
pub type MILLI = BoundTag<'l'>;
pub type SECOND = BoundTag<'s'>;
pub type MINUTE = BoundTag<'m'>;
pub type HOUR = BoundTag<'h'>;
pub type DAY = BoundTag<'d'>;

// we have a limitation to const types, so we better use the ones defined.
fn fill_duration_measure<T: TimeTag>() -> Result<String, impl Display> {
    match T::inner() {
        v @ ('d' | 'h' | 'm' | 's') => Ok(v.to_string()),
        'l' => Ok("ms".to_string()),
        'n' => Ok("ns".to_string()),
        other => Err(format!("'{other}' measure not supported")),
    }
}

impl<'de, const MIN_DURATION: usize, TAG: TimeTag> Deserialize<'de>
    for MinimalBoundedDuration<MIN_DURATION, TAG>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: Duration = Duration::deserialize(deserializer)?;
        let reformat_measure = fill_duration_measure::<TAG>().map_err(Error::custom)?;
        let parsed_duration =
            humantime::parse_duration(format!("{MIN_DURATION}{reformat_measure}").as_str())
                .map_err(Error::custom)?;
        let min_duration: Duration = Duration::new(
            parsed_duration
                .as_secs()
                .try_into()
                .map_err(Error::custom)?,
            parsed_duration
                .subsec_nanos()
                .try_into()
                .map_err(Error::custom)?,
        );
        if value < min_duration {
            return Err(Error::custom(format!(
                "Minimal duration is {min_duration} but got {value}"
            )));
        }
        Ok(Self {
            duration: value,
            _tag: PhantomData,
        })
    }
}

impl<const MIN_DURATION: usize, TAG: TimeTag> From<MinimalBoundedDuration<MIN_DURATION, TAG>>
    for Duration
{
    fn from(value: MinimalBoundedDuration<MIN_DURATION, TAG>) -> Self {
        value.duration
    }
}

#[cfg(test)]
mod test {
    use crate::bounded_duration::{MinimalBoundedDuration, DAY, SECOND};
    use time::Duration;

    #[test]
    fn success_deserialize() {
        let value = serde_json::to_value(Duration::days(1)).unwrap();
        let _duration: MinimalBoundedDuration<1, SECOND> = serde_json::from_value(value).unwrap();
    }

    #[test]
    #[should_panic]
    fn fail_deserialize_with_type_bound() {
        let value = serde_json::to_value(Duration::seconds(1)).unwrap();

        let _duration: MinimalBoundedDuration<1, DAY> = serde_json::from_value(value).unwrap();
    }
}
