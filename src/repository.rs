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

impl Chunk {
    /// Will Pack the data into a chunk with the given compression and encryption
    pub fn pack(
        data: &[u8],
        compression: Compression,
        encryption: Encryption,
        key: &[u8],
    ) -> Chunk {
        let compressed_data = compression.compress(data);
        let data = encryption.encrypt(&compressed_data, key);
        Chunk {
            data,
            compression,
            encryption,
        }
    }

    /// Decrypts and decompresses the data in the chunk
    ///
    /// Will return none if either the decompression or the decryption fail
    pub fn unpack(&self, key: &[u8]) -> Option<Vec<u8>> {
        let decrypted_data = self.encryption.decrypt(&self.data, key)?;
        let decompressed_data = self.compression.decompress(&decrypted_data)?;

        Some(decompressed_data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::prelude::*;
    use std::str;

    #[test]
    fn chunk_aes256cbc_zstd6() {
        let data_string =
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";

        let data_bytes = data_string.as_bytes();
        let compression = Compression::ZStd { level: 6 };
        let encryption = Encryption::new_aes256cbc();

        let mut key: [u8; 32] = [0; 32];
        thread_rng().fill_bytes(&mut key);

        let packed = Chunk::pack(&data_bytes, compression, encryption, &key);

        let output_bytes = packed.unpack(&key).unwrap();
        let output_string = str::from_utf8(&output_bytes).unwrap();

        assert_eq!(data_string, output_string);
    }

}
