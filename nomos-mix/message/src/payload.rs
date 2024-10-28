// Lioness encryption is used, as Sphinx does.
// `lioness` crate requires the old version (0.8) of `blake2` crate
// and the `chacha` crate that is not being maintained actively.
// TODO: Update the `lioness` crate on our own, or consider another encryption algorithm
// (e.g. AES-CTR, just ChaCha20, or AEZ).
use blake2::VarBlake2b;
use chacha::ChaCha;
use lioness::Lioness;

use crate::Error;

pub fn encrypt_payload(payload: &mut [u8], key: &[u8; lioness::RAW_KEY_SIZE]) -> Result<(), Error> {
    let cipher = Lioness::<VarBlake2b, ChaCha>::new_raw(key);
    cipher.encrypt(payload)?;
    Ok(())
}

pub fn decrypt_payload(payload: &mut [u8], key: &[u8; lioness::RAW_KEY_SIZE]) -> Result<(), Error> {
    let cipher = Lioness::<VarBlake2b, ChaCha>::new_raw(key);
    cipher.decrypt(payload)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_and_decrypt() {
        let key = [2u8; lioness::RAW_KEY_SIZE];
        let payload = [3u8; lioness::RAW_KEY_SIZE * 2];

        let mut encrypted = payload;
        encrypt_payload(&mut encrypted, &key).unwrap();
        assert_ne!(encrypted, payload);

        let mut decrypted = encrypted;
        decrypt_payload(&mut decrypted, &key).unwrap();
        assert_eq!(decrypted, payload);
    }

    #[test]
    fn decrypt_with_wrong_key() {
        let mut key = [2u8; lioness::RAW_KEY_SIZE];
        let payload = [3u8; lioness::RAW_KEY_SIZE * 2];

        let mut encrypted = payload;
        encrypt_payload(&mut encrypted, &key).unwrap();
        assert_ne!(encrypted, payload);

        // Manipulate the key
        key[0] += 1;

        let mut decrypted = encrypted;
        decrypt_payload(&mut decrypted, &key).unwrap();
        assert_ne!(decrypted, payload);
    }
}
