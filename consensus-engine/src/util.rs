pub(crate) mod serde_array32 {
    use serde::{Deserialize, Serialize};
    use std::cell::RefCell;

    const MAX_SERIALIZATION_LENGTH: usize = 32 * 2 + 2;

    thread_local! {
        static STRING_BUFFER: RefCell<String> = RefCell::new(String::with_capacity(MAX_SERIALIZATION_LENGTH));
    }

    pub fn serialize<S: serde::Serializer>(t: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            let size = MAX_SERIALIZATION_LENGTH;
            STRING_BUFFER.with(|s| {
                let mut s = s.borrow_mut();
                s.clear();
                s.reserve(size);
                s.push_str("0x");
                for v in t {
                    std::fmt::write(&mut *s, format_args!("{:02x}", v)).unwrap();
                }
                s.serialize(serializer)
            })
        } else {
            t.serialize(serializer)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            <&str>::deserialize(deserializer).and_then(try_from_str::<D>)
        } else {
            let x = <&[u8]>::deserialize(deserializer)?;
            <[u8; 32]>::try_from(x).map_err(<D::Error as serde::de::Error>::custom)
        }
    }

    fn try_from_str<'de, D>(mut s: &str) -> Result<[u8; 32], D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Debug, thiserror::Error)]
        enum DecodeError {
            #[error("expected str of length {expected} but got length {actual}")]
            UnexpectedSize { expected: usize, actual: usize },
            #[error("invalid character pair '{0}{1}'")]
            InvalidCharacter(char, char),
        }

        // check if we start with 0x or not
        let prefix_len = if s.starts_with("0x") || s.starts_with("0X") {
            s = &s[2..];
            2
        } else {
            0
        };

        if s.len() != 32 * 2 {
            return Err(<D::Error as serde::de::Error>::custom(
                DecodeError::UnexpectedSize {
                    expected: 32 * 2 + prefix_len,
                    actual: s.len(),
                },
            ));
        }

        let mut output = [0; 32];

        for (chars, byte) in s.as_bytes().chunks_exact(2).zip(output.iter_mut()) {
            let (l, r) = (chars[0] as char, chars[1] as char);
            match (l.to_digit(16), r.to_digit(16)) {
                (Some(l), Some(r)) => *byte = (l as u8) << 4 | r as u8,
                (_, _) => {
                    return Err(<D::Error as serde::de::Error>::custom(
                        DecodeError::InvalidCharacter(l, r),
                    ))
                }
            };
        }
        Ok(output)
    }
}
