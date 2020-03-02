// In this case, this lint results in harder to read code for security critical portions
#![allow(clippy::match_same_arms)]

// We are going to be allowing unused imports and unused variables a lot in this module, to make the
// code a bit cleaner. We write the code assuming that the user will compile with at least one
// encryption method (this is an encrypting archiver after all)

#[cfg(feature = "aes")]
use aes::Aes256;
#[cfg(feature = "aes-ctr")]
use aes_ctr::Aes256Ctr;
#[allow(unused_imports)]
use block_modes::block_padding::Pkcs7;
#[allow(unused_imports)]
use block_modes::{BlockMode, Cbc};
#[cfg(feature = "chacha20")]
use chacha20::ChaCha20;
use rand::prelude::*;
#[allow(unused_imports)]
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::cmp;
#[allow(unused_imports)]
use stream_cipher::generic_array::GenericArray;
#[allow(unused_imports)]
use stream_cipher::{NewStreamCipher, SyncStreamCipher};
use thiserror::Error;
#[allow(unused_imports)]
use zeroize::Zeroize;

use crate::repository::Key;

/// Error describing things that can go wrong with encryption/decryption
#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("Invalid key/IV length used to construct encryptor/decryptor")]
    InvalidKeyIVLength(#[from] block_modes::InvalidKeyIvLength),
    #[error("Error with block mode encryption/decryption")]
    BlockModeError(#[from] block_modes::BlockModeError),
}

type Result<T> = std::result::Result<T, EncryptionError>;

/// Tag for the encryption algorthim and IV used by a particular chunk
#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Encryption {
    NoEncryption,
    AES256CBC { iv: [u8; 16] },
    AES256CTR { iv: [u8; 16] },
    ChaCha20 { iv: [u8; 12] },
}

impl Encryption {
    /// Creates an AES256CBC with a random, securely generated IV
    pub fn new_aes256cbc() -> Encryption {
        let mut iv: [u8; 16] = [0; 16];
        thread_rng().fill_bytes(&mut iv);
        Encryption::AES256CBC { iv }
    }

    /// Creates a new AES256CTR with a random securely generated IV
    pub fn new_aes256ctr() -> Encryption {
        let mut iv: [u8; 16] = [0; 16];
        thread_rng().fill_bytes(&mut iv);
        Encryption::AES256CTR { iv }
    }

    /// Creates a new ChaCha20 with a random securely generated IV
    pub fn new_chacha20() -> Encryption {
        let mut iv: [u8; 12] = [0; 12];
        thread_rng().fill_bytes(&mut iv);
        Encryption::ChaCha20 { iv }
    }

    /// Returns the key length of this encryption method in bytes
    ///
    /// NoEncryption has a key length of 16 bytes, as some things rely on a non-zero key
    /// length.
    pub fn key_length(&self) -> usize {
        match self {
            Encryption::NoEncryption => 16,
            Encryption::AES256CBC { .. } => 32,
            Encryption::AES256CTR { .. } => 32,
            Encryption::ChaCha20 { .. } => 32,
        }
    }

    /// Encrypts a bytestring using the algrothim specified in the tag, and the
    /// given key.
    ///
    /// Still requires a key in the event of no encryption, but it does not read this
    /// key, so any value can be used. Will pad key with zeros if it is too short
    ///
    /// Will panic on encryption failure
    pub fn encrypt(&self, data: &[u8], key: &Key) -> Vec<u8> {
        self.encrypt_bytes(data, key.key())
    }

    /// Internal method that does the actual encryption, please use the encrypt method
    /// to avoid key confusion
    #[allow(unused_variables)]
    pub fn encrypt_bytes(&self, data: &[u8], key: &[u8]) -> Vec<u8> {
        match self {
            Encryption::NoEncryption => data.to_vec(),
            Encryption::AES256CBC { iv } => {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "aes")] {
                        // Create a key of the correct length, and fill it with
                        // zeros to start with
                        let mut proper_key: [u8; 32] = [0; 32];
                        proper_key[..cmp::min(key.len(), 32)]
                            .clone_from_slice(&key[..cmp::min(key.len(), 32)]);

                        let encryptor: Cbc<Aes256, Pkcs7> = Cbc::new_var(&proper_key, &iv[..]).unwrap();
                        let final_result = encryptor.encrypt_vec(data);
                        // Zeroize key
                        proper_key.zeroize();

                        final_result
                    } else {
                        unimplemented!("Asuran has not been compiled with AES support.")
                    }

                }
            }
            Encryption::AES256CTR { iv } => {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "aes-ctr")] {
                        let mut proper_key: [u8; 32] = [0; 32];
                        proper_key[..cmp::min(key.len(), 32)]
                            .clone_from_slice(&key[..cmp::min(key.len(), 32)]);
                        let key = GenericArray::from_slice(&key);
                        let iv = GenericArray::from_slice(&iv[..]);
                        let mut encryptor = Aes256Ctr::new(&key, &iv);
                        let mut final_result = data.to_vec();
                        encryptor.apply_keystream(&mut final_result);

                        proper_key.zeroize();
                        final_result
                    } else {
                        unimplemented!("Asuran has not been compiled with AES-CTR Support")
                    }
                }
            }
            Encryption::ChaCha20 { iv } => {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "chacha20")] {
                        let mut proper_key: [u8; 32] = [0; 32];
                        proper_key[..cmp::min(key.len(), 32)]
                            .clone_from_slice(&key[..cmp::min(key.len(), 32)]);
                        let key = GenericArray::from_slice(&key);
                        let iv = GenericArray::from_slice(&iv[..]);
                        let mut encryptor = ChaCha20::new(&key, &iv);
                        let mut final_result = data.to_vec();
                        encryptor.apply_keystream(&mut final_result);

                        proper_key.zeroize();
                        final_result
                    } else {
                        unimplemented!("Asuran has not been compiled with ChaCha20 support")
                    }
                }
            }
        }
    }

    /// Decrypts a bytestring with the given key
    ///
    /// Still requires a key in the event of no encryption, but it does not read this key,
    /// so any value can be used. Will pad key with zeros if it is too short.
    ///
    /// Will return None on encryption failure
    pub fn decrypt(&self, data: &[u8], key: &Key) -> Result<Vec<u8>> {
        self.decrypt_bytes(data, key.key())
    }

    #[allow(unused_variables)]
    pub fn decrypt_bytes(&self, data: &[u8], key: &[u8]) -> Result<Vec<u8>> {
        match self {
            Encryption::NoEncryption => Ok(data.to_vec()),
            Encryption::AES256CBC { iv } => {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "aes")] {
                        // Creates a key of the correct length, and fills it with
                        // zeros to start with
                        let mut proper_key: [u8; 32] = [0; 32];
                        // Copy key into proper key
                        proper_key[..cmp::min(key.len(), 32)]
                            .clone_from_slice(&key[..cmp::min(key.len(), 32)]);

                        let decryptor: Cbc<Aes256, Pkcs7> = Cbc::new_var(&key, &iv[..])?;
                        let final_result = decryptor.decrypt_vec(data)?;

                        // Zeroize key
                        proper_key.zeroize();

                        Ok(final_result)
                    } else {
                        unimplemented!("Asuran has not been compiled with AES support")
                    }
                }
            }
            Encryption::AES256CTR { iv } => {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "aes-ctr")] {
                        let mut proper_key: [u8; 32] = [0; 32];
                        proper_key[..cmp::min(key.len(), 32)]
                            .clone_from_slice(&key[..cmp::min(key.len(), 32)]);

                        let key = GenericArray::from_slice(&key);
                        let iv = GenericArray::from_slice(&iv[..]);
                        let mut decryptor = Aes256Ctr::new(&key, &iv);
                        let mut final_result = data.to_vec();
                        decryptor.apply_keystream(&mut final_result);

                        proper_key.zeroize();
                        Ok(final_result)
                    } else {
                        unimplemented!("Asuran has not been compiled with AES support")
                    }
                }
            }
            Encryption::ChaCha20 { iv } => {
                cfg_if::cfg_if! {
                    if #[cfg(feature = "chacha20")] {
                        let mut proper_key: [u8; 32] = [0; 32];
                        proper_key[..cmp::min(key.len(), 32)]
                            .clone_from_slice(&key[..cmp::min(key.len(), 32)]);

                        let key = GenericArray::from_slice(&key);
                        let iv = GenericArray::from_slice(&iv[..]);
                        let mut decryptor = ChaCha20::new(&key, &iv);
                        let mut final_result = data.to_vec();
                        decryptor.apply_keystream(&mut final_result);

                        proper_key.zeroize();
                        Ok(final_result)
                    } else {
                        unimplemented!("Asuran has not been compiled with ChaCha20 support")
                    }
                }
            }
        }
    }

    /// Conviencence function to get a new tag from an old one, specifying the
    /// same algorithim, but with a new, securely generated IV
    pub fn new_iv(self) -> Encryption {
        match self {
            Encryption::NoEncryption => Encryption::NoEncryption,
            Encryption::AES256CBC { .. } => Encryption::new_aes256cbc(),
            Encryption::AES256CTR { .. } => Encryption::new_aes256ctr(),
            Encryption::ChaCha20 { .. } => Encryption::new_chacha20(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str;

    fn test_encryption(enc: Encryption) {
        let mut key: [u8; 32] = [0; 32];
        thread_rng().fill_bytes(&mut key);

        let data_string =
            "The quick brown fox jumps over the lazy dog. Jackdaws love my big sphinx of quartz.";
        let encrypted_string = enc.encrypt_bytes(data_string.as_bytes(), &key);
        let decrypted_bytes = enc.decrypt_bytes(&encrypted_string, &key).unwrap();
        let decrypted_string = str::from_utf8(&decrypted_bytes).unwrap();

        println!("Input string: {}", data_string);
        println!("Input bytes: \n{:X?}", data_string.as_bytes());
        println!("Encrypted bytes: \n{:X?}", encrypted_string);
        println!("Decrypted bytes: \n{:X?}", decrypted_bytes);
        println!("Decrypted string: {}", decrypted_string);

        assert_eq!(data_string, decrypted_string);
    }

    #[test]
    fn test_aes256cbc() {
        let enc = Encryption::new_aes256cbc();
        test_encryption(enc);
    }

    #[test]
    fn test_aes256ctr() {
        let enc = Encryption::new_aes256ctr();
        test_encryption(enc);
    }
}
