use crate::repository::Encryption;
use crypto::scrypt::*;
use rand::prelude::*;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};

/// Stores the encryption key used by the archive
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct Key {
    // TODO: Store multiple keys for various processes that require them
    key: Vec<u8>,
}

impl Key {
    /// Creates a key from the given array of bytes
    pub fn from_bytes(bytes: &[u8]) -> Key {
        Key {
            key: bytes.to_vec(),
        }
    }

    /// Securely generates a random key
    ///
    /// accepts the length in bytes they key should be
    pub fn random(length: usize) -> Key {
        let mut buffer = vec![0; length];
        thread_rng().fill_bytes(&mut buffer);
        Key { key: buffer }
    }

    /// Obtains a refrence to the key bytes
    pub fn key(&self) -> &[u8] {
        &self.key
    }
}

/// Stores the key, encrypted with another key dervied from the user specified
/// password/passphrase
///
/// Uses scrypt to derive the key
///
/// Uses a 32 byte salt that is randomly generated
#[derive(Serialize, Deserialize, Clone)]
pub struct EncryptedKey {
    encrypted_bytes: Vec<u8>,
    salt: [u8; 32],
    scrypt_n: u8,
    scrypt_r: u32,
    scrypt_p: u32,
    encryption: Encryption,
}

impl EncryptedKey {
    /// Produces an encrypted key from the specified userkey and encryption method
    pub fn encrypt(
        key: &Key,
        encryption: Encryption,
        user_key: &[u8],
        scrypt_n: u8,
        scrypt_r: u32,
        scrypt_p: u32,
    ) -> EncryptedKey {
        // get a fresh IV for the encryption method
        let encryption = encryption.new_iv();
        // Serialize the key
        let mut key_buffer = Vec::<u8>::new();
        key.serialize(&mut Serializer::new(&mut key_buffer))
            .unwrap();
        // Generate a salt
        let mut salt = [0; 32];
        thread_rng().fill_bytes(&mut salt);
        // Produce a key from the user key
        let params = ScryptParams::new(scrypt_n, scrypt_r, scrypt_p);
        let mut generated_key_bytes = vec![0; encryption.key_length()];
        scrypt(user_key, &salt, &params, &mut generated_key_bytes);
        // Encrypt the key using the derived key
        let encrypted_bytes = encryption.encrypt(&key_buffer, &generated_key_bytes);

        EncryptedKey {
            encrypted_bytes,
            salt,
            scrypt_n,
            scrypt_p,
            scrypt_r,
            encryption,
        }
    }

    /// Convience function that uses scrypt paramaters that the author of this
    /// programs believes are reasonable as of time of writing. Please review
    /// them and apply your own common sense before blaming the author for the
    /// FBI reading your data.
    ///
    /// Paramaters are:
    ///  - N: 32768 (passed in as 15, as the scrypt function uses the log_2 of n)
    ///  - r: 8
    ///  - p: 1
    pub fn encrypt_defaults(key: &Key, encryption: Encryption, user_key: &[u8]) -> EncryptedKey {
        EncryptedKey::encrypt(key, encryption, user_key, 15, 8, 1)
    }

    /// Attempts to decrypt the key using the provided user key
    ///
    /// Will return none on failure
    pub fn decrypt(&self, user_key: &[u8]) -> Option<Key> {
        // Derive the key from the user key
        let params = ScryptParams::new(self.scrypt_n, self.scrypt_r, self.scrypt_p);
        let mut generated_key_bytes = vec![0; self.encryption.key_length()];
        scrypt(user_key, &self.salt, &params, &mut generated_key_bytes);
        // Decrypt the key
        let key_bytes = self
            .encryption
            .decrypt(&self.encrypted_bytes, &generated_key_bytes)?;
        // Deserialize the key
        let mut de = Deserializer::new(&key_bytes[..]);
        let key: Key = Deserialize::deserialize(&mut de).ok()?;

        Some(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt() {
        let input_key = Key::random(8);
        let user_key = "A secure password".as_bytes();
        let encryption = Encryption::new_aes256ctr();
        let enc_key = EncryptedKey::encrypt_defaults(&input_key, encryption, user_key);
        let output_key = enc_key.decrypt(user_key).unwrap();

        assert_eq!(input_key, output_key);
    }
}
