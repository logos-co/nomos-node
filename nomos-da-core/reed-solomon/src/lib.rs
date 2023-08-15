use reed_solomon_erasure::{galois_8::ReedSolomon, Error};

/// Encode a vector of elements into a polynomial and evaluate it over the domain
///  # Arguments
/// * `domain` - Domain with the extension factor for the elements
/// * `elements` - Elements to encode
pub fn encode_elements(parity_ratio: usize, elements: &[u8]) -> Result<Vec<u8>, Error> {
    let mut encoded = vec![vec![0]; elements.len() * (parity_ratio + 1)];
    for (i, &e) in elements.iter().enumerate() {
        encoded[i] = vec![e; 1];
    }
    let encoder = ReedSolomon::new(elements.len(), elements.len() * parity_ratio)?;
    encoder.encode(&mut encoded)?;
    Ok(encoded.into_iter().flatten().collect())
}

pub fn decode_from_elements(elements: &[u8]) -> Vec<u8> {
    vec![]
}

#[cfg(test)]
mod test {
    #[test]
    fn encode_with_size() {
        let elements = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let encoded = super::encode_elements(1, &elements).unwrap();
        // check intended size
        assert_eq!(encoded.len(), 16);
        // check elements
        assert_eq!(&encoded[0..8], &elements);
    }
}
