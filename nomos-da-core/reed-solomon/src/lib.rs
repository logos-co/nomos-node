use reed_solomon_erasure::{galois_8::ReedSolomon, Error};

/// Reed Sololomon encode the elements with a custom parity ratio
///  # Arguments
/// * `parity_ratio` - Ratio of parity elements over original elements size
/// * `elements` - Elements to encode
pub fn encode_elements(parity_ratio: usize, elements: &[u8]) -> Result<Vec<u8>, Error> {
    let mut encoded = vec![vec![0]; elements.len() * (parity_ratio + 1)];
    for (i, &e) in elements.iter().enumerate() {
        // review bytes encoding
        encoded[i] = e.to_be_bytes().to_vec();
    }
    let encoder = ReedSolomon::new(elements.len(), elements.len() * parity_ratio)?;
    encoder.encode(&mut encoded)?;
    Ok(encoded.into_iter().flatten().collect())
}

/// Reed solomon decode the elements with a custom parity ratio
/// # Arguments
/// * `original_size` - Original size of encoded elements
/// * `parity_ratio` - Ratio of parity elements over original elements size (must be the same as the one used for encoding)
/// * `elements` - Elements to decode
pub fn decode_from_elements(
    original_size: usize,
    parity_ratio: usize,
    elements: &[Option<u8>],
) -> Result<Vec<u8>, Error> {
    let mut elements: Vec<_> = elements
        .iter()
        .map(|e| e.map(|n| n.to_be_bytes().to_vec()))
        .collect();
    let decoder = ReedSolomon::new(original_size, parity_ratio * original_size)?;
    decoder.reconstruct(&mut elements)?;
    Ok(elements
        .into_iter()
        .filter_map(|e: Option<Vec<u8>>| e.map(|n| u8::from_be_bytes(n.try_into().unwrap())))
        .collect())
}

#[cfg(test)]
mod test {
    use reed_solomon_erasure::Error;

    #[test]
    fn encode_with_ratio() {
        let elements = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let encoded = super::encode_elements(1, &elements).unwrap();
        // check intended size
        assert_eq!(encoded.len(), 16);
        // check elements
        assert_eq!(&encoded[0..8], &elements);
    }

    #[test]
    fn decode_with_ratio() {
        let elements = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let encoded = super::encode_elements(1, &elements).unwrap();
        let mut encoded: Vec<_> = encoded.into_iter().map(Some).collect();
        encoded[4..12].copy_from_slice(&[None; 8]);
        let decoded = super::decode_from_elements(8, 1, &encoded).unwrap();
        assert_eq!(decoded[0..8], elements);
    }

    #[test]
    fn decode_fails_with_insufficient_shards() {
        let elements = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let encoded = super::encode_elements(1, &elements).unwrap();
        let mut encoded: Vec<_> = encoded.into_iter().map(Some).collect();
        encoded[7..].copy_from_slice(&[None; 9]);
        let decoded = super::decode_from_elements(8, 1, &encoded);
        assert!(matches!(decoded, Err(Error::TooFewShardsPresent)));
    }
}
