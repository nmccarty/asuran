/*!
This module contains chunks for describing and interacting with cryptographic
key material
*/
use crate::repository::Encryption;

use argon2::{self, Config, ThreadMode, Variant, Version};
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use serde_cbor::{de::from_slice, Serializer};
use thiserror::Error;
use tracing::{error, trace};
use zeroize::Zeroize;

use std::convert::TryInto;

/// Error describing things that can go wrong with key handling
#[derive(Error, Debug)]
pub enum KeyError {
    #[error("Encrypted key encryption/decryption failed")]
    EncryptionError(#[from] super::EncryptionError),
    #[error("Something went wrong with argon2")]
    Argon2Error(#[from] argon2::Error),
    #[error("Something went wrong with Serialization/Deserailization")]
    DecodeError(#[from] serde_cbor::error::Error),
}

type Result<T> = std::result::Result<T, KeyError>;

/// Stores the Key material used by an asuran repository.
///
/// Contains 5 separate pieces of key material:
///
/// - `key`:
///
/// The key used for encryption/decryption operations
///
/// - `hmac_key`:
///
/// The key used for generating the integrity validation HMAC tag
///
/// - `id_key`:
///
/// The key used for `ChunkID` generation using the HMAC
///
/// - `chunker_nonce`:
///
/// A random `u64` used for chunker randomization with supported chunking algorithms
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
    /// Will split the key stream into thirds.
    ///
    /// Does not perform any padding.
    pub fn from_bytes(bytes: &[u8], chunker_nonce: u64) -> Key {
        let mut buffer1 = Vec::new();
        let mut buffer2 = Vec::new();
        let mut buffer3 = Vec::new();
        for (i, byte) in bytes.iter().enumerate() {
            match i % 3 {
                0 => buffer1.push(*byte),
                1 => buffer2.push(*byte),
                2 => buffer3.push(*byte),
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

    /// Securely generates a random bundle of key material
    ///
    /// Takes the desired length in bytes of each individual key component
    #[tracing::instrument(level = "trace")]
    pub fn random(length: usize) -> Key {
        let mut buffer1 = vec![0; length];
        thread_rng().fill_bytes(&mut buffer1);
        let mut buffer2 = vec![0; length];
        thread_rng().fill_bytes(&mut buffer2);
        let mut buffer3 = vec![0; length];
        thread_rng().fill_bytes(&mut buffer3);
        trace!("Generated a random key");
        Key {
            key: buffer1,
            hmac_key: buffer2,
            id_key: buffer3,
            chunker_nonce: thread_rng().next_u64(),
        }
    }

    /// Obtains a reference to the key bytes
    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// Obtains a reference to the HMAC key bytes
    pub fn hmac_key(&self) -> &[u8] {
        &self.hmac_key
    }

    /// Obtains a reference to the ID key bytes
    pub fn id_key(&self) -> &[u8] {
        &self.id_key
    }

    /// Obtains the chunker nonce
    pub fn chunker_nonce(&self) -> u64 {
        self.chunker_nonce
    }
}

/// Stores the key, encrypted with another key derived from the user specified
/// password/passphrase
///
/// Uses argon2 to derive the key encryption key from the user supplied key.
///
/// Uses a 32 byte salt that is randomly generated
///
/// Currently uses semi-arbitrary defaults for some values. TODO: allow configuration of this
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EncryptedKey {
    encrypted_bytes: Vec<u8>,
    salt: [u8; 32],
    mem_cost: u32,
    time_cost: u32,
    encryption: Encryption,
}

impl EncryptedKey {
    /// Produces an encrypted key from the specified user key and encryption method
    #[tracing::instrument(level = "trace")]
    pub fn encrypt(
        key: &Key,
        mem_cost: u32,
        time_cost: u32,
        mut encryption: Encryption,
        user_key: &[u8],
    ) -> EncryptedKey {
        // Serialize the key
        let mut key_buffer = Vec::<u8>::new();
        // Since were are serializing to a Vec::<u8>, and Key does not contain any types that
        // can fail to serialize, this call to unwrap should be infallible
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
            hash_length: encryption
                .key_length()
                .try_into()
                .expect("Key length was too large (larger than usize)"),
        };

        let generated_key_bytes = argon2::hash_raw(&user_key, &salt, &config)
            .expect("Unable to hash password with argon2, most likely due to invalid settings.");
        let encrypted_bytes = encryption.encrypt_bytes(&key_buffer, &generated_key_bytes);
        trace!("Encrypted key");
        EncryptedKey {
            encrypted_bytes,
            salt,
            mem_cost,
            time_cost,
            encryption,
        }
    }

    /// Convince function that uses argon2 parameters that the author of this program
    /// believes are reasonable as of time of writing. Please review them and apply your
    /// own common sense before blaming the author for the FBI reading your data.
    ///
    /// Parameters are:
    /// - `mem_cost`: 65536
    /// - `time_cost`: 10
    #[cfg_attr(tarpaulin, skip)]
    #[tracing::instrument(level = "trace")]
    pub fn encrypt_defaults(key: &Key, encryption: Encryption, user_key: &[u8]) -> EncryptedKey {
        trace!("Encrypting key with default settings");
        EncryptedKey::encrypt(key, 65536, 10, encryption, user_key)
    }

    /// Attempts to decrypt the key material using the user supplied key.
    ///
    /// # Errors:
    ///
    /// Will return `Err(KeyError)` if key decryption fails
    #[tracing::instrument(level = "error")]
    pub fn decrypt(&self, user_key: &[u8]) -> Result<Key> {
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
            hash_length: self
                .encryption
                .key_length()
                .try_into()
                .expect("Key length was too large (larger than usize)"),
        };
        let generated_key_bytes = argon2::hash_raw(&user_key, &self.salt, &config)?;
        // Decrypt the key
        let key_bytes = self
            .encryption
            .decrypt_bytes(&self.encrypted_bytes, &generated_key_bytes)?;
        // Deserialize the key
        let key = from_slice(&key_bytes[..])?;

        Ok(key)
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
        let enc_key = EncryptedKey::encrypt(&input_key, 1024, 2, encryption, user_key);
        let output_key = enc_key.decrypt(user_key).unwrap();

        assert_eq!(input_key, output_key);
    }

    #[test]
    fn from_bytes() {
        let input = [1, 2, 3, 1, 2, 3, 1, 2, 3];
        let key = Key::from_bytes(&input, 4);
        assert_eq!(key.key, [1, 1, 1]);
        assert_eq!(key.hmac_key, [2, 2, 2]);
        assert_eq!(key.id_key, [3, 3, 3]);
        assert_eq!(key.chunker_nonce(), 4);
    }
}
