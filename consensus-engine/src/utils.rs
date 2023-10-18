#[cfg(feature = "serde")]
pub(crate) mod serde {
    fn serialize_human_readable_bytes_array<const N: usize, S: serde::Serializer>(
        src: [u8; N],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        use serde::Serialize;
        use std::fmt::Write;
        let size = N * 2 + 2;
        let mut s = String::with_capacity(size);
        s.push_str("0x");
        for v in src {
            // unwrap is safe because we allocate enough space
            write!(&mut s, "{:02x}", v).unwrap();
        }
        s.serialize(serializer)
    }

    pub(crate) fn serialize_bytes_array<const N: usize, S: serde::Serializer>(
        src: [u8; N],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            serialize_human_readable_bytes_array(src, serializer)
        } else {
            serializer.serialize_bytes(&src)
        }
    }

    fn deserialize_human_readable_bytes_array<'de, const N: usize, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; N], D::Error> {
        use serde::Deserialize;

        let mut s = <&str>::deserialize(deserializer)?;
        let extra_len = if s.starts_with("0x") || s.starts_with("0X") {
            s = &s[2..];
            2
        } else {
            0
        };
        if s.len() != N * 2 {
            return Err(<D::Error as serde::de::Error>::invalid_length(
                N + extra_len,
                &format!("{N}").as_str(),
            ));
        }

        let mut output = [0u8; N];

        for (chars, byte) in s.as_bytes().chunks_exact(2).zip(output.iter_mut()) {
            let (l, r) = (chars[0] as char, chars[1] as char);
            match (l.to_digit(16), r.to_digit(16)) {
                (Some(l), Some(r)) => *byte = (l as u8) << 4 | r as u8,
                (_, _) => {
                    return Err(<D::Error as serde::de::Error>::custom(format!(
                        "Invalid character pair '{l}{r}'"
                    )))
                }
            };
        }
        Ok(output)
    }

    fn deserialize_human_unreadable_bytes_array<
        'de,
        const N: usize,
        D: serde::Deserializer<'de>,
    >(
        deserializer: D,
    ) -> Result<[u8; N], D::Error> {
        use serde::Deserialize;
        <&[u8]>::deserialize(deserializer).and_then(|bytes| {
            if bytes.len() != N {
                Err(<D::Error as serde::de::Error>::invalid_length(
                    bytes.len(),
                    &format!("{N}").as_str(),
                ))
            } else {
                let mut output = [0u8; N];
                output.copy_from_slice(bytes);
                Ok(output)
            }
        })
    }

    pub(crate) fn deserialize_bytes_array<'de, const N: usize, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; N], D::Error> {
        if deserializer.is_human_readable() {
            deserialize_human_readable_bytes_array(deserializer)
        } else {
            deserialize_human_unreadable_bytes_array(deserializer)
        }
    }
}
