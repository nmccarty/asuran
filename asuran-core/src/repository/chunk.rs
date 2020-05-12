/*!
The Chunk is the lowest level of abstraction in an asuran repository.

Chunks are raw binary blobs, optionally compressed and encrypted, and keyed by
an HMAC of their plain text contents, and tagged with an HMAC of their encrypted
contents (different keys are used for both HMACs).

They can contain any arbitrary sequence of bytes.
*/
use super::{Compression, Encryption, Key, HMAC};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use std::cmp;

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

/// Key used for indexing a `Chunk` in a repository
///
/// These are usually derived via an HMAC of the chunks plain text, and are used for
/// reduplication. If two chunks have the same `ChunkID`, it is assumed that they
/// are identical.
#[derive(PartialEq, Eq, Copy, Clone, Serialize, Deserialize, Hash, Debug)]
pub struct ChunkID {
    /// Keys are a bytestring of length 32
    ///
    /// This lines up well with SHA256 and other 256 bit hashes. Longer hashes will be
    /// truncated and shorter ones (not reccomended) will be padded with zeros at the
    /// end.
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

    /// Provides a reference to a key's raw bytes
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

    /// Returns a random id, used for testing
    pub fn random_id() -> ChunkID {
        let id = rand::random();
        ChunkID { id }
    }
}

/// Encapsulates the Encryption, Compression, and HMAC tags for a chunk
#[derive(Serialize, Deserialize, Clone, Debug, Copy, PartialEq, Eq)]
pub struct ChunkSettings {
    pub compression: Compression,
    pub encryption: Encryption,
    pub hmac: HMAC,
}

impl ChunkSettings {
    /// Returns a `ChunkSettings` with `Encryption::NoEncryption`,
    /// `Compression::NoCompression`, and `HMAC::Blake2b`.
    ///
    /// These settings are, very nearly, the least computationally intensive that asuran
    /// supports.
    pub fn lightweight() -> ChunkSettings {
        ChunkSettings {
            compression: Compression::NoCompression,
            encryption: Encryption::NoEncryption,
            hmac: HMAC::Blake2b,
        }
    }
}

/// A split representation of a `Chunk`'s 'header' or metadata.
/// Used for on disk storage
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChunkHeader {
    compression: Compression,
    encryption: Encryption,
    hmac: HMAC,
    mac: Vec<u8>,
    id: ChunkID,
}

/// A split representation of a `Chunk`'s body, or contained data
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChunkBody(pub Vec<u8>);

/// A binary blob, ready to be commited to storage
///
/// A `Chunk` is an arbitrary sequence of bytes, along with its associated `ChunkID`
/// key.
///
/// Data in a `Chunk` has already undergone any selected compression and encryption,
/// and has an associated HMAC tag used for verifying the integrity of the data.
/// This HMAC tag is unrelated to the `ChunkID` key, and uses a separate HMAC key.
///
/// Chunks are additionally tagged with the encryption and compression modes used
/// for them.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Chunk {
    /// The data of the chunk, stored as a vec of raw bytes
    #[serde(with = "serde_bytes")]
    data: Vec<u8>,
    /// Compression algorithim used
    compression: Compression,
    /// Encryption Algorithim used, also stores IV
    encryption: Encryption,
    /// HMAC algorithim used
    hmac: HMAC,
    /// HMAC tag of the cyphertext bytes of this chunk
    #[serde(with = "serde_bytes")]
    mac: Vec<u8>,
    /// `ChunkID`, used for indexing in the repository and deduplication
    id: ChunkID,
}

impl Chunk {
    /// Produces a `Chunk` from the given data, using the specified
    /// encryption, and hmac algorithms, as well as the supplied key material.
    ///
    /// # Panics
    ///
    /// Will panic if any of the compression, encryption, or `HMAC` operations fail.
    /// This would represent a massive programming oversight which the user of the
    /// library has little hope of recovering from safely without compromising
    /// cryptographic integrity.
    pub fn pack(
        data: Vec<u8>,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: &Key,
    ) -> Chunk {
        let id_mac = hmac.id(&data, key);
        let id = ChunkID::new(&id_mac);
        Chunk::pack_with_id(data, compression, encryption, hmac, key, id)
    }

    /// Constructs a `Chunk` from its raw parts.
    ///
    /// This has potentially dangerous consequences if done incorrectly, and should be
    /// avoided if another method is available.
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

    /// Produces a `Chunk` using the provided settings, but overriding the `ChunkID`
    /// key.
    ///
    /// This has the potential to do serious damage to a repository if used incorrectly,
    /// and should be avoided if another method is available.
    pub fn pack_with_id(
        data: Vec<u8>,
        compression: Compression,
        mut encryption: Encryption,
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

    /// Validates, decrypts, and decompresses the data in a `Chunk`.
    ///
    /// # Errors
    ///
    /// Will return `Err(HMACVailidationFailed)` if the chunk fails validation.
    ///
    /// Will return `Err(EncryptionError)` if decryption fails.
    ///
    /// Will return `Err(CompressionError)` if decompression fails.
    ///
    /// All of these error values indicate that the `Chunk` is corrupted or otherwise
    /// malformed.
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
    /// Returns the length of the data in the `Chunk`
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

    /// Returns the `ChunkID` key assocaited with the data in this chunk.
    pub fn get_id(&self) -> ChunkID {
        self.id
    }

    /// Returns the `mac` value of this chunk
    pub fn mac(&self) -> Vec<u8> {
        self.mac.clone()
    }

    /// Splits a `Chunk` into its header and body components
    pub fn split(self) -> (ChunkHeader, ChunkBody) {
        let header = ChunkHeader {
            compression: self.compression,
            encryption: self.encryption,
            hmac: self.hmac,
            mac: self.mac,
            id: self.id,
        };
        let body = ChunkBody(self.data);

        (header, body)
    }

    /// Combines a header and a body into a `Chunk`
    pub fn unsplit(header: ChunkHeader, body: ChunkBody) -> Chunk {
        Chunk {
            data: body.0,
            compression: header.compression,
            encryption: header.encryption,
            hmac: header.hmac,
            mac: header.mac,
            id: header.id,
        }
    }

    /// Returns a copy of the encryption method/iv used for the chunk
    pub fn encryption(&self) -> Encryption {
        self.encryption
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

impl PartialEq for Chunk {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.data == other.data
    }
}

impl Eq for Chunk {}

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

        let output_bytes = packed.unpack(&key).expect("Failed to unpack output bytes");

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

    #[test]
    fn split_unsplit() {
        let data_string = "I am but a humble test string";
        let data_bytes = data_string.as_bytes().to_vec();
        let compression = Compression::LZ4 { level: 1 };
        let encryption = Encryption::new_aes256ctr();
        let hmac = HMAC::SHA256;

        let key = Key::random(32);

        let packed = Chunk::pack(data_bytes, compression, encryption, hmac, &key);
        let (header, body) = packed.split();
        let packed = Chunk::unsplit(header, body);

        let result = packed.unpack(&key);

        assert!(result.is_ok());
    }
}
