//! Data structures for describing chunks of data
//!
//! Contains structs representing both encrypted and unencrypted data

use super::{Compression, Encryption, Key, HMAC};
use serde::{Deserialize, Serialize};
use std::cmp;
use thiserror::Error;

/// Error for all the various things that can go wrong with handling chunks
#[derive(Error, Debug)]
pub enum ChunkError {
    #[error("Compression Error")]
    CompressionError(#[from] super::CompressionError),
    #[error("Encryption Error")]
    EncryptionError(#[from] super::EncryptionError),
    #[error("Key Error")]
    KeyError(#[from] super::KeyError),
    #[error("HMAC Vailidation Failed")]
    HMACValidationFailed,
}

type Result<T> = std::result::Result<T, ChunkError>;

/// Key for an object in a repository
#[derive(PartialEq, Eq, Copy, Clone, Serialize, Deserialize, Hash, Debug)]
pub struct ChunkID {
    /// Keys are a bytestring of length 32
    ///
    /// This lines up well with SHA256 and other 256 bit hashes.
    /// Longer hashes will be truncated and shorter ones (not reccomended) will be padded.
    id: [u8; 32],
}

impl ChunkID {
    /// Will create a new key from a slice.
    ///
    /// Keys longer than 32 bytes will be truncated.
    /// Keys shorter than 32 bytes will be padded at the end with zeros.
    pub fn new(input_id: &[u8]) -> ChunkID {
        let mut id: [u8; 32] = [0; 32];
        id[..cmp::min(32, input_id.len())]
            .clone_from_slice(&input_id[..cmp::min(32, input_id.len())]);
        ChunkID { id }
    }

    /// Returns an immutable refrence to the key in bytestring form
    #[cfg_attr(tarpaulin, skip)]
    pub fn get_id(&self) -> &[u8] {
        &self.id
    }

    /// Verifies equaliy of this key with the first 32 bytes of a slice
    pub fn verify(&self, slice: &[u8]) -> bool {
        if slice.len() < self.id.len() {
            false
        } else {
            let mut equal = true;
            for (i, val) in self.id.iter().enumerate() {
                if *val != slice[i] {
                    equal = false;
                }
            }
            equal
        }
    }

    /// Returns the special all-zero key used for the manifest
    pub fn manifest_id() -> ChunkID {
        ChunkID { id: [0_u8; 32] }
    }
}

/// Chunk Settings
///
/// Encapsulates the Encryption, Compression, and HMAC tags for a chunk
#[derive(Serialize, Deserialize, Clone, Debug, Copy, PartialEq, Eq)]
pub struct ChunkSettings {
    pub compression: Compression,
    pub encryption: Encryption,
    pub hmac: HMAC,
}

impl ChunkSettings {
    /// Returns a chunksettings with no compression, no encryption, and blake2b
    pub fn lightweight() -> ChunkSettings {
        ChunkSettings {
            compression: Compression::NoCompression,
            encryption: Encryption::NoEncryption,
            hmac: HMAC::Blake2b,
        }
    }
}

/// A raw block of data and its associated `ChunkID`
///
/// This data is not encrypted, compressed, or otherwise tampered with, and can not be directly
/// inserted into the repo.
pub struct UnpackedChunk {
    data: Vec<u8>,
    id: ChunkID,
}

impl UnpackedChunk {
    /// Creates a new unpacked chunk
    ///
    /// HMAC algorthim used for chunkid is specified by chunksettings
    ///
    /// Key used for ChunkID generation is determined by key
    pub fn new(data: Vec<u8>, settings: ChunkSettings, key: &Key) -> UnpackedChunk {
        let id = settings.hmac.id(data.as_slice(), &key);
        let cid = ChunkID::new(&id);
        UnpackedChunk { data, id: cid }
    }

    /// Returns the chunkid
    pub fn id(&self) -> ChunkID {
        self.id
    }

    /// Returns a refrence to the data
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Returns the data consuming self
    pub fn consuming_data(self) -> Vec<u8> {
        self.data
    }
}

/// Data chunk
///
/// Encrypted, compressed object, to be stored in the repository
#[derive(Serialize, Deserialize, Debug)]
pub struct Chunk {
    /// The data of the chunk, stored as a vec of raw bytes
    #[serde(with = "serde_bytes")]
    data: Vec<u8>,
    /// Compression algorithim used
    compression: Compression,
    /// Encryption Algorithim used, also stores IV
    encryption: Encryption,
    /// HMAC algorithim used
    ///
    /// HAMC key is also the same as the repo encryption key
    hmac: HMAC,
    /// Actual MAC value of this chunk
    #[serde(with = "serde_bytes")]
    mac: Vec<u8>,
    /// Chunk ID, generated from the HMAC
    id: ChunkID,
}

impl Chunk {
    /// Will Pack the data into a chunk with the given compression and encryption
    pub fn pack(
        data: Vec<u8>,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: &Key,
    ) -> Chunk {
        let id_mac = hmac.id(&data, key);
        let compressed_data = compression.compress(data);
        let data = encryption.encrypt(&compressed_data, key);
        let id = ChunkID::new(&id_mac);
        let mac = hmac.mac(&data, key);
        Chunk {
            data,
            compression,
            encryption,
            hmac,
            mac,
            id,
        }
    }

    /// Constructs a chunk from its parts
    pub fn from_parts(
        data: Vec<u8>,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        mac: Vec<u8>,
        id: ChunkID,
    ) -> Chunk {
        Chunk {
            data,
            compression,
            encryption,
            hmac,
            mac,
            id,
        }
    }

    /// Will pack a chunk, but manually setting the id instead of hashing
    ///
    /// This function should be used carefully, as it has potentiall to do major damage to the repository
    pub fn pack_with_id(
        data: Vec<u8>,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: &Key,
        id: ChunkID,
    ) -> Chunk {
        let compressed_data = compression.compress(data);
        let data = encryption.encrypt(&compressed_data, key);
        let mac = hmac.mac(&data, key);
        Chunk {
            data,
            compression,
            encryption,
            hmac,
            mac,
            id,
        }
    }

    /// Decrypts and decompresses the data in the chunk
    ///
    /// Will return none if either the decompression or the decryption fail
    ///
    /// Will also return none if the HMAC verification fails
    pub fn unpack(&self, key: &Key) -> Result<Vec<u8>> {
        if self.hmac.verify_hmac(&self.mac, &self.data, key) {
            let decrypted_data = self.encryption.decrypt(&self.data, key)?;
            let decompressed_data = self.compression.decompress(decrypted_data)?;

            Ok(decompressed_data)
        } else {
            Err(ChunkError::HMACValidationFailed)
        }
    }

    #[cfg_attr(tarpaulin, skip)]
    /// Returns the length of the data bytes
    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[cfg_attr(tarpaulin, skip)]
    /// Determine if this chunk is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    #[cfg_attr(tarpaulin, skip)]
    /// Returns a reference to the raw bytes of this chunk
    pub fn get_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Gets the key for this block
    pub fn get_id(&self) -> ChunkID {
        self.id
    }

    #[cfg(test)]
    #[cfg_attr(tarpaulin, skip)]
    /// Testing only function used to corrupt the data
    pub fn break_data(&mut self, index: usize) {
        let val = self.data[index];
        if val == 0 {
            self.data[index] = 1;
        } else {
            self.data[index] = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk_with_settings(compression: Compression, encryption: Encryption, hmac: HMAC) {
        let data_string =
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";

        let data_bytes = data_string.as_bytes().to_vec();
        println!("Data: \n:{:X?}", data_bytes);
        println!("{:?} {:?} {:?}", compression, hmac, encryption);

        let key = Key::random(32);
        let packed = Chunk::pack(data_bytes, compression, encryption, hmac, &key);

        let output_bytes = packed.unpack(&key).unwrap();

        assert_eq!(data_string.as_bytes().to_vec(), output_bytes);
    }

    #[test]
    fn all_combos() {
        let compressions = [
            Compression::NoCompression,
            Compression::ZStd { level: 1 },
            Compression::LZ4 { level: 1 },
            Compression::LZMA { level: 1 },
        ];
        let encryptions = [
            Encryption::NoEncryption,
            Encryption::new_aes256cbc(),
            Encryption::new_aes256ctr(),
            Encryption::new_chacha20(),
        ];
        let hmacs = [
            HMAC::SHA256,
            HMAC::Blake2b,
            HMAC::Blake2bp,
            HMAC::Blake3,
            HMAC::SHA3,
        ];
        for c in compressions.iter() {
            for e in encryptions.iter() {
                for h in hmacs.iter() {
                    chunk_with_settings(*c, *e, *h);
                }
            }
        }
    }

    #[test]
    fn detect_bad_data() {
        let data_string = "I am but a humble test string";
        let data_bytes = data_string.as_bytes().to_vec();
        let compression = Compression::NoCompression;
        let encryption = Encryption::NoEncryption;
        let hmac = HMAC::SHA256;

        let key = Key::random(32);

        let mut packed = Chunk::pack(data_bytes, compression, encryption, hmac, &key);
        packed.break_data(5);

        let result = packed.unpack(&key);

        assert!(result.is_err());
    }

    #[test]
    fn chunk_id_equality() {
        let data1 = [1_u8; 64];
        let data2 = [2_u8; 64];
        let id = ChunkID::new(&data1);
        assert!(id.verify(&data1));
        assert!(!id.verify(&data2));
    }
}
