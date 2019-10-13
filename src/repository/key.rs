use crate::repository::Encryption;
use argon2::{self, Config, ThreadMode, Variant, Version};
use rand::prelude::*;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

/// Stores the encryption key used by the archive
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Zeroize)]
#[zeroize(drop)]
pub struct Key {
    key: Vec<u8>,
    hmac_key: Vec<u8>,
    id_key: Vec<u8>,
    chunker_nonce: u64,
}

impl Key {
    /// Creates a key from the given array of bytes
    ///
    /// Will split the key stream into thirds
    pub fn from_bytes(bytes: &[u8], chunker_nonce: u64) -> Key {
        let mut buffer1 = Vec::new();
        let mut buffer2 = Vec::new();
        let mut buffer3 = Vec::new();
        for (i, byte) in bytes.iter().enumerate() {
            match i % 3 {
                0 => buffer1.push(*byte),
                1 => buffer2.push(*byte),
                3 => buffer3.push(*byte),
                _ => unreachable!(),
            };
        }
        Key {
            key: buffer1,
            hmac_key: buffer2,
            id_key: buffer3,
            chunker_nonce,
        }
    }

    /// Securely generates a random key
    ///
    /// Takes the desired length in bytes of each individual key
    pub fn random(length: usize) -> Key {
        let mut buffer1 = vec![0; length];
        thread_rng().fill_bytes(&mut buffer1);
        let mut buffer2 = vec![0; length];
        thread_rng().fill_bytes(&mut buffer2);
        let mut buffer3 = vec![0; length];
        thread_rng().fill_bytes(&mut buffer3);
        Key {
            key: buffer1,
            hmac_key: buffer2,
            id_key: buffer3,
            chunker_nonce: thread_rng().next_u64(),
        }
    }

    /// Obtains a refrence to the key bytes
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// Obtains a refrence to the HMAC key bytes
    pub fn hmac_key(&self) -> &[u8] {
        &self.hmac_key
    }

    /// Obtains a refrence to the ID key bytes
    pub fn id_key(&self) -> &[u8] {
        &self.id_key
    }

    /// Obtains the chunker nonce
    pub fn chunker_nonce(&self) -> u64 {
        self.chunker_nonce
    }
}

/// Stores the key, encrypted with another key dervied from the user specified
/// password/passphrase
///
/// Uses argon2 to derive the key
///
/// Uses a 32 byte salt that is randomly generated
///
/// Currently uses semi-arbitrary defaults for some values. TODO: allow configuration of this
#[derive(Serialize, Deserialize, Clone)]
pub struct EncryptedKey {
    encrypted_bytes: Vec<u8>,
    salt: [u8; 32],
    mem_cost: u32,
    time_cost: u32,
    encryption: Encryption,
}

impl EncryptedKey {
    /// Produces an encrypted key from the specified userkey and encryption method
    pub fn encrypt(
        key: &Key,
        mem_cost: u32,
        time_cost: u32,
        encryption: Encryption,
        user_key: &[u8],
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
        let config = Config {
            variant: Variant::Argon2id,
            version: Version::Version13,
            mem_cost,
            time_cost,
            thread_mode: ThreadMode::Sequential,
            lanes: 1,
            secret: &[],
            ad: &[],
            hash_length: encryption.key_length() as u32,
        };

        let generated_key_bytes = argon2::hash_raw(&user_key, &salt, &config).unwrap();
        let encrypted_bytes = encryption.encrypt_bytes(&key_buffer, &generated_key_bytes);

        EncryptedKey {
            encrypted_bytes,
            salt,
            mem_cost,
            time_cost,
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
        EncryptedKey::encrypt(key, 65536, 10, encryption, user_key)
    }

    /// Attempts to decrypt the key using the provided user key
    ///
    /// Will return none on failure
    pub fn decrypt(&self, user_key: &[u8]) -> Option<Key> {
        // Derive the key from the user key
        let config = Config {
            variant: Variant::Argon2id,
            version: Version::Version13,
            mem_cost: self.mem_cost,
            time_cost: self.time_cost,
            thread_mode: ThreadMode::Sequential,
            lanes: 1,
            secret: &[],
            ad: &[],
            hash_length: self.encryption.key_length() as u32,
        };
        let generated_key_bytes = argon2::hash_raw(&user_key, &self.salt, &config).unwrap();
        // Decrypt the key
        let key_bytes = self
            .encryption
            .decrypt_bytes(&self.encrypted_bytes, &generated_key_bytes)?;
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
