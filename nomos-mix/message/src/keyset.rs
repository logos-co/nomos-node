use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::StaticSecret;

pub const STREAM_CIPHER_KEY_SIZE: usize = 16;
pub const INTEGRITY_MAC_KEY_SIZE: usize = 16;
pub const STATIC_SECRET_SIZE: usize = 32;
pub const KEYSET_LENGTH: usize =
    STREAM_CIPHER_KEY_SIZE + INTEGRITY_MAC_KEY_SIZE + lioness::RAW_KEY_SIZE + STATIC_SECRET_SIZE;

/// A set of keys to perform one layer of encryption on a packet.
/// A recipient derives this to decrypt the layer of encryption.
/// See [`KeySet::derive_all`] for details.
#[derive(Clone)]
pub struct KeySet {
    /// A key to encrypt/decrypt packet header using AES128-CTR.
    pub _stream_cipher_key: [u8; STREAM_CIPHER_KEY_SIZE],
    /// A key to calculate HMAC of the encrypted packet header.
    pub _header_integrity_hmac_key: [u8; INTEGRITY_MAC_KEY_SIZE],
    /// A key to encrypt payload using Lioness.
    pub payload_lioness_key: [u8; lioness::RAW_KEY_SIZE],
    /// To derive the new ephemeral public key to be shared to the next recipient.
    /// See [`KeySet::derive_all`] for details.
    pub ephemeral_private_key: StaticSecret,
}

impl KeySet {
    /// Derive a [`KeySet`] from a 32-byte shared secret
    /// by expanding it to 256-byte and splitting it into 4 parts.
    pub(crate) fn derive(shared_secret: &[u8; 32]) -> Self {
        let expanded_key = Self::expand_shared_secret(shared_secret);

        let mut i = 0;

        let mut stream_cipher_key = [0u8; STREAM_CIPHER_KEY_SIZE];
        stream_cipher_key.copy_from_slice(&expanded_key[i..i + STREAM_CIPHER_KEY_SIZE]);
        i += STREAM_CIPHER_KEY_SIZE;

        let mut header_integrity_hmac_key = [0u8; INTEGRITY_MAC_KEY_SIZE];
        header_integrity_hmac_key.copy_from_slice(&expanded_key[i..i + INTEGRITY_MAC_KEY_SIZE]);
        i += INTEGRITY_MAC_KEY_SIZE;

        let mut payload_lioness_key = [0u8; lioness::RAW_KEY_SIZE];
        payload_lioness_key.copy_from_slice(&expanded_key[i..i + lioness::RAW_KEY_SIZE]);
        i += lioness::RAW_KEY_SIZE;

        let mut ephemeral_private_key = [0u8; STATIC_SECRET_SIZE];
        ephemeral_private_key.copy_from_slice(&expanded_key[i..i + STATIC_SECRET_SIZE]);

        Self {
            _stream_cipher_key: stream_cipher_key,
            _header_integrity_hmac_key: header_integrity_hmac_key,
            payload_lioness_key,
            ephemeral_private_key: StaticSecret::from(ephemeral_private_key),
        }
    }

    /// Expand a 32-byte shared secret to 256-byte using HKDF with SHA-256.
    fn expand_shared_secret(shared_secret: &[u8; 32]) -> [u8; KEYSET_LENGTH] {
        let mut expanded_key = [0u8; KEYSET_LENGTH];
        Hkdf::<Sha256>::new(None, shared_secret)
            .expand(&[], &mut expanded_key)
            .unwrap();
        expanded_key
    }

    /// Derive multiple [`KeySet`]s from a list of public keys and an initial ephemeral private key
    /// by accumulated Diffie-Hellman operations.
    ///
    /// These [`KeySet`]s can be used by a packet builder
    /// to perform layered encryption on the packet for multiple recipients.
    /// Each recipient should derive its [`KeySet`] to decrypt one layer of encryption.
    ///
    /// To allow each recipient to derive its shared secret and [`KeySet`],
    /// the naive way may be including all ephemeral public keys
    /// for each recipient in the packet header.
    /// Instead, this method uses the accumulated DH operations
    /// to allow each recipient to derive the new ephemeral public key for the next recipient.
    /// Doing so, only one ephemeral public key is needed to be included in the packet header.
    ///
    /// The public key of the initial ephemeral private key should be shared to the first recipient.
    ///
    /// Example:
    /// As a packet builder:
    ///     shared1 = DH(pub1, epriv0)
    ///     keyset1 = KeySet(shared1)
    ///     shared2 = DH(DH(pub2, epriv0), keyset1.epriv)
    ///     keyset2 = KeySet(shared2)
    /// As the 1st recipient:
    ///     shared1 = DH(epub0, priv1)
    ///     keyset1 = KeySet(shared1)
    ///     epub1 = DH(epub0, keyset1.epriv)
    /// As the 2nd recipient:
    ///     shared2 = DH(priv2, epub0) = DH(priv2, DH(epub0, keyset1.epriv))
    ///     keyset2 = KeySet(shared2)
    ///
    pub fn derive_all(
        recipient_pubkeys: &[x25519_dalek::PublicKey],
        initial_ephemeral_privkey: &x25519_dalek::StaticSecret,
    ) -> Vec<KeySet> {
        let mut keysets = Vec::with_capacity(recipient_pubkeys.len());

        let mut ephemeral_privkeys = vec![initial_ephemeral_privkey.clone()];

        // For each recipient, derive a shared secret by accumulated DH operations
        // and derive a [`KeySet`] from the shared secret.
        recipient_pubkeys
            .iter()
            .cloned()
            .for_each(|recipient_pubkey| {
                // Derive a shared secret of the recipient by accumulated DH operations.
                let shared_secret = ephemeral_privkeys
                    .iter()
                    .fold(recipient_pubkey, |accumulated_pubkey, ephemeral_privkey| {
                        let shared_secret = ephemeral_privkey.diffie_hellman(&accumulated_pubkey);
                        // Convert SharedSecret to PublicKey just for the next DH operation.
                        // This is just converting the type, not manipulating bytes.
                        x25519_dalek::PublicKey::from(shared_secret.to_bytes())
                    })
                    .to_bytes(); // Convert the final PublicKey back to the bytes for SharedSecret.

                // Derive a [`KeySet`] from the shared secret
                // and keep the derived new ephemeral private key for the next recipient.
                let keyset = KeySet::derive(&shared_secret);
                ephemeral_privkeys.push(keyset.ephemeral_private_key.clone());
                keysets.push(keyset);
            });

        keysets
    }

    /// Derive a ephemeral public key to be shared to the next recipient
    /// by the Diffie-Hellman operation using the shared public key
    /// and the derived ephemeral private key of the current recipient.
    ///
    /// This is the operation symmetric to the one of accumulated DH operations
    /// performed in the [`KeySet::derive_all`].
    pub fn derive_next_ephemeral_public_key(
        &self,
        shared_ephemeral_pubkey: &x25519_dalek::PublicKey,
    ) -> x25519_dalek::PublicKey {
        let new_shared_secert = self
            .ephemeral_private_key
            .diffie_hellman(shared_ephemeral_pubkey);
        x25519_dalek::PublicKey::from(new_shared_secert.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // For the security reasons, implement PartialEq and Debug for KeySet only for testing.
    impl PartialEq for KeySet {
        fn eq(&self, other: &Self) -> bool {
            self._stream_cipher_key == other._stream_cipher_key
                && self._header_integrity_hmac_key == other._header_integrity_hmac_key
                && self.payload_lioness_key == other.payload_lioness_key
                && self.ephemeral_private_key.to_bytes() == other.ephemeral_private_key.to_bytes()
        }
    }

    impl std::fmt::Debug for KeySet {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("KeySet")
                .field("stream_cipher_key", &self._stream_cipher_key)
                .field(
                    "header_integrity_hmac_key",
                    &self._header_integrity_hmac_key,
                )
                .field("payload_lioness_key", &self.payload_lioness_key)
                .field(
                    "ephemeral_private_key",
                    &self.ephemeral_private_key.to_bytes(),
                )
                .finish()
        }
    }

    #[test]
    fn test_keyset_derivation_from_builder_and_recipients() {
        // Prepare keys of recipients
        let recipient_privkeys = (0..2)
            .map(|_| x25519_dalek::StaticSecret::random())
            .collect::<Vec<_>>();
        let recipient_pubkeys = recipient_privkeys
            .iter()
            .map(x25519_dalek::PublicKey::from)
            .collect::<Vec<_>>();

        // A packet builder derives all [`KeySet`]s.
        let ephemeral_privkey = x25519_dalek::StaticSecret::random();
        let ephemeral_pubkey = x25519_dalek::PublicKey::from(&ephemeral_privkey);
        let keysets = KeySet::derive_all(&recipient_pubkeys, &ephemeral_privkey);
        assert_eq!(keysets.len(), recipient_pubkeys.len());

        // To store [`KeySet`]s derived by the recipients
        let mut keysets_by_recipients = Vec::with_capacity(keysets.len());

        // The recipient 1 derives the shared secret by DH key exchange and derives the [`KeySet`].
        let shared_secret = recipient_privkeys[0].diffie_hellman(&ephemeral_pubkey);
        let keyset = KeySet::derive(shared_secret.as_bytes());
        // Derive the next ephemeral public key
        let ephemeral_pubkey = keyset.derive_next_ephemeral_public_key(&ephemeral_pubkey);
        keysets_by_recipients.push(keyset);

        // The recipient 2 derives the shared secret by DH key exchange and derives the [`KeySet`].
        let shared_secret = recipient_privkeys[1].diffie_hellman(&ephemeral_pubkey);
        let keyset = KeySet::derive(shared_secret.as_bytes());
        keysets_by_recipients.push(keyset);

        // The [`KeySet`]s derived by the recipients must be equal to
        // the ones derived by the packet builder.
        assert_eq!(keysets, keysets_by_recipients);
    }

    #[test]
    fn test_keyset_derivation_with_wrong_recipient_private_keys() {
        // Prepare keys of recipients
        let recipient_privkeys = (0..2)
            .map(|_| x25519_dalek::StaticSecret::random())
            .collect::<Vec<_>>();
        let recipient_pubkeys = recipient_privkeys
            .iter()
            .map(x25519_dalek::PublicKey::from)
            .collect::<Vec<_>>();

        // A packet builder derives all [`KeySet`]s.
        let ephemeral_privkey = x25519_dalek::StaticSecret::random();
        let ephemeral_pubkey = x25519_dalek::PublicKey::from(&ephemeral_privkey);
        let keysets = KeySet::derive_all(&recipient_pubkeys, &ephemeral_privkey);
        assert_eq!(keysets.len(), recipient_pubkeys.len());

        // To store [`KeySet`]s derived by the recipients
        let mut keysets_by_recipients = Vec::with_capacity(keysets.len());

        // The recipient 1 derives the shared secret with a wrong private key
        let shared_secret = x25519_dalek::StaticSecret::random().diffie_hellman(&ephemeral_pubkey);
        let keyset = KeySet::derive(shared_secret.as_bytes());
        // Derive the next ephemeral public key
        let ephemeral_pubkey = keyset.derive_next_ephemeral_public_key(&ephemeral_pubkey);
        keysets_by_recipients.push(keyset);

        // The recipient 2 derives the shared secret with a wrong private key
        let shared_secret = x25519_dalek::StaticSecret::random().diffie_hellman(&ephemeral_pubkey);
        let keyset = KeySet::derive(shared_secret.as_bytes());
        keysets_by_recipients.push(keyset);

        // The [`KeySet`]s derived by the recipients must be NOT equal to
        // the ones derived by the packet builder.
        assert_ne!(keysets, keysets_by_recipients);
    }
}
