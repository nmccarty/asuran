use crypto::buffer::{BufferResult, ReadBuffer, WriteBuffer};
use crypto::{aes, blockmodes, buffer};
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp;
use zeroize::Zeroize;

/// Encryption Algorithim
#[derive(Copy, Clone, Serialize, Deserialize)]
pub enum Encryption {
    AES256CBC { iv: [u8; 16] },
    NoEncryption,
}

impl Encryption {
    /// Creates an AES256CBC with a random, securely generated IV
    pub fn new_aes256cbc() -> Encryption {
        let mut iv: [u8; 16] = [0; 16];
        thread_rng().fill_bytes(&mut iv);
        Encryption::AES256CBC { iv }
    }

    /// Returns the key length of this encryption method in bytes
    pub fn key_length(&self) -> usize {
        match self {
            Encryption::NoEncryption => 0,
            Encryption::AES256CBC { .. } => 32,
        }
    }

    /// Encrypts a bytestring with the appropiate algortihim with the given key
    ///
    /// Still requires a key in the event of no encryption, but it does not read this
    /// key, so any value can be used. Will pad key with zeros if it is too short
    ///
    /// Will panic on encryption failure
    pub fn encrypt(&self, data: &[u8], key: &[u8]) -> Vec<u8> {
        match self {
            Encryption::NoEncryption => data.to_vec(),
            Encryption::AES256CBC { iv } => {
                // Create a key of the correct length, and fill it with
                // zeros to start with
                let mut proper_key: [u8; 32] = [0; 32];
                proper_key[..cmp::min(key.len(), 32)]
                    .clone_from_slice(&key[..cmp::min(key.len(), 32)]);

                let mut encryptor = aes::cbc_encryptor(
                    aes::KeySize::KeySize256,
                    &proper_key,
                    iv,
                    blockmodes::PkcsPadding,
                );

                let mut final_result = Vec::new();
                let mut read_buffer = buffer::RefReadBuffer::new(data);
                let mut buffer = [0; 4096];
                let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);

                loop {
                    let result = encryptor
                        .encrypt(&mut read_buffer, &mut write_buffer, true)
                        .unwrap();
                    final_result.extend(
                        write_buffer
                            .take_read_buffer()
                            .take_remaining()
                            .iter()
                            .cloned(),
                    );

                    match result {
                        BufferResult::BufferUnderflow => break,
                        BufferResult::BufferOverflow => {}
                    }
                }

                // Zeroize key
                proper_key.zeroize();

                final_result
            }
        }
    }

    /// Decrypts a bytestring with the given key
    ///
    /// Still requires a key in the event of no encryption, but it does not read this key,
    /// so any value can be used. Will pad key with zeros if it is too short.
    ///
    /// Will return None on encryption failure
    pub fn decrypt(&self, data: &[u8], key: &[u8]) -> Option<Vec<u8>> {
        match self {
            Encryption::NoEncryption => Some(data.to_vec()),
            Encryption::AES256CBC { iv } => {
                // Creates a key of the correct length, and fills it with
                // zeros to start with
                let mut proper_key: [u8; 32] = [0; 32];
                // Copy key into proper key
                proper_key[..cmp::min(key.len(), 32)]
                    .clone_from_slice(&key[..cmp::min(key.len(), 32)]);

                let mut decryptor = aes::cbc_decryptor(
                    aes::KeySize::KeySize256,
                    &proper_key,
                    iv,
                    blockmodes::PkcsPadding,
                );

                let mut final_result = Vec::<u8>::new();
                let mut read_buffer = buffer::RefReadBuffer::new(data);
                let mut buffer = [0; 4096];
                let mut write_buffer = buffer::RefWriteBuffer::new(&mut buffer);

                loop {
                    let result = decryptor.decrypt(&mut read_buffer, &mut write_buffer, true);
                    match result {
                        Err(_) => {
                            return {
                                proper_key.zeroize();
                                None
                            }
                        }
                        Ok(result) => {
                            final_result.extend(
                                write_buffer
                                    .take_read_buffer()
                                    .take_remaining()
                                    .iter()
                                    .cloned(),
                            );
                            match result {
                                BufferResult::BufferUnderflow => break,
                                BufferResult::BufferOverflow => {}
                            }
                        }
                    }
                }

                // Zeroize key
                proper_key.zeroize();

                Some(final_result)
            }
        }
    }

    pub fn new_iv(self) -> Encryption {
        match self {
            Encryption::NoEncryption => Encryption::NoEncryption,
            Encryption::AES256CBC { .. } => Encryption::new_aes256cbc(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str;

    #[test]
    fn test_aes256cbc() {
        let mut key: [u8; 32] = [0; 32];
        thread_rng().fill_bytes(&mut key);

        let enc = Encryption::new_aes256cbc();

        let data_string =
            "The quick brown fox jumps over the lazy dog. Jackdaws love my big sphinx of quartz.";
        let encrypted_string = enc.encrypt(data_string.as_bytes(), &key);
        let decrypted_bytes = enc.decrypt(&encrypted_string, &key).unwrap();
        let decrypted_string = str::from_utf8(&decrypted_bytes).unwrap();;

        println!("Input string: {}", data_string);
        println!("Input bytes: \n{:X?}", data_string.as_bytes());
        println!("Encrypted bytes: \n{:X?}", encrypted_string);
        println!("Decrypted bytes: \n{:X?}", decrypted_bytes);
        println!("Decrypted string: {}", decrypted_string);

        assert_eq!(data_string, decrypted_string);
    }
}
