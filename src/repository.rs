use std::cmp;

use crate::repository::backend::*;
use crate::repository::compression::*;
use crate::repository::encryption::*;

pub mod backend;
pub mod compression;
pub mod encryption;

pub struct Repository {
    backend: Box<dyn Backend>,
}

/// HMAC Algorithim
pub enum HMAC {
    SHA256,
}

/// Key for an object in a repository
pub struct Key {
    /// Keys are a bytestring of length 32
    ///
    /// This lines up well with SHA256 and other 256 bit hashes.
    /// Longer hashes will be truncated and shorter ones (not reccomended) will be padded.
    key: [u8; 32],
}

impl Key {
    /// Will create a new key from a slice.
    ///
    /// Keys longer than 32 bytes will be truncated.
    /// Keys shorter than 32 bytes will be padded at the end with zeros.
    pub fn new(input_key: &[u8]) -> Key {
        let mut key: [u8; 32] = [0; 32];
        for i in 0..cmp::min(32, input_key.len()) {
            key[i] = input_key[i];
        }
        Key { key }
    }

    /// Returns an immutable refrence to the key in bytestring form
    pub fn get_key(&self) -> &[u8] {
        &self.key
    }
}

/// Data chunk
///
/// Encrypted, compressed object, to be stored in the repository
pub struct Chunk {
    /// The data of the chunk, stored as a vec of raw bytes
    data: Vec<u8>,
    /// Compression algorithim used
    compression: Compression,
    /// Encryption Algorithim used, also stores IV
    encryption: Encryption,
}
