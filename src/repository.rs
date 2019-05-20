//! The repository imeplements a low-level key-value store, upon which all
//! higher level structures in asuran are built.
//!
//! The repository stores individual chunks, arrays of bytes, that can be
//! compressed and encrypted. Chunks are addressed by their key, which,
//! with the exception of the repository manifest, is derived from an HMAC of
//! the plain text of the chunk.
//!
//! Asuran repositories currently only operate in append only mode
//!
//! # Encryption and Compression
//!
//! Encryption and compression algorthims can be swapped out on a chunk by
//! chunk basis, with Encryption::NoEncryption and Compression::NoCompression
//! providing pass through modes for those who do not wish to use those
//! features.
//!
//! # Authentication
//!
//! Asuran uses Hash based Method Authentication Codes (HMAC), with swappable
//! hash algorithims, for both deduplicating and ensuring data integrety.
//!
//! The hash algorhtim used for the HMAC can also be changed out on a chunk by
//! chunk basis, though this would not be wise to do. As deduplication is
//! perfomed based on plaintext HMAC, this would severely compromise the
//! effectiveness of deduplicaiton.
//!
//! While the hash algrorithim used for HMAC can be swapped out, unlike the
//! ones for encryption and compression, it can not be turned off. Asuran
//! always verifies the intergety of the data.
//!
//! # Deduplication
//!
//! The deduplication strategy in asuran is straight foward. Each chunk is
//! stored in the repository with the hash of its plaintext as its key.
//! As the hash function used is a cryptographically secure HMAC, we can be
//! sure within the limits of reason that if two chunks have the same key,
//! they have the same data, and if they have the same data, then they have the
//! same key.
//!
//! Asuran will not write a chunk whose key already exists in the repository,
//! effectivly preventing the storage of duplicate chunks.

use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::cmp;
use std::collections::HashMap;

pub use crate::repository::backend::filesystem::FileSystem;
pub use crate::repository::backend::Backend;
pub use crate::repository::compression::Compression;
pub use crate::repository::encryption::Encryption;
pub use crate::repository::hmac::HMAC;

#[cfg(feature = "profile")]
use flamer::*;

pub mod backend;
pub mod compression;
pub mod encryption;
pub mod hmac;

/// Provides an interface to the storage-backed key value store
///
/// File access is abstracted behind a swappable backend, all backends should
/// use roughly the same format, but leeway is made for cases such as S3 having
/// a flat directory structure
pub struct Repository {
    backend: Box<dyn Backend>,
    index: HashMap<Key, (u64, u64, u64)>,
    /// Default compression for new chunks
    compression: Compression,
    /// Default MAC algorthim for new chunks
    hmac: HMAC,
    /// Default encryption algorthim for new chunks
    encryption: Encryption,
    /// Encryption key for this repo
    key: Vec<u8>,
}

impl Repository {
    /// Creates a new repository with the specificed backend and defaults
    pub fn new(
        backend: Box<dyn Backend>,
        compression: Compression,
        hmac: HMAC,
        encryption: Encryption,
        key: &[u8],
    ) -> Repository {
        // Check for index, create a new one if it doesnt exist
        let index_vec = backend.get_index();
        let index = if index_vec.is_empty() {
            HashMap::new()
        } else {
            let mut de = Deserializer::new(index_vec.as_slice());
            Deserialize::deserialize(&mut de).expect("Unable to parse index")
        };
        let key = key.to_vec();

        Repository {
            backend,
            index,
            compression,
            hmac,
            encryption,
            key,
        }
    }

    #[cfg_attr(feature = "profile", flame)]
    /// Commits the index to storage
    ///
    /// This should be called every time an archive or manifest is written, at
    /// the very least
    pub fn commit_index(&self) {
        let mut buff = Vec::<u8>::new();
        let mut se = Serializer::new(&mut buff);
        self.index.serialize(&mut se).unwrap();
        self.backend
            .write_index(&buff)
            .expect("Unable to commit index");
    }

    #[cfg_attr(feature = "profile", flame)]
    /// Writes a chunk directly to the repository
    ///
    /// Will return (Chunk_Id, Already_Present)
    ///
    /// Already_Present will be true if the chunk already exists in the
    /// repository.
    pub fn write_raw(&mut self, chunk: Chunk) -> Option<(Key, bool)> {
        let id = chunk.get_id();

        // Check if chunk exists
        if self.has_chunk(id) {
            Some((id, true))
        } else {
            let mut buff = Vec::<u8>::new();
            chunk.serialize(&mut Serializer::new(&mut buff)).unwrap();

            // Get highest segment and check to see if has enough space
            let backend = &self.backend;
            let mut seg_id = backend.highest_segment();
            let test_segment = backend.get_segment(seg_id);
            // If no segments exist, we must create one
            let test_segment = if test_segment.is_none() {
                seg_id = backend.make_segment()?;
                backend.get_segment(seg_id)?
            } else {
                test_segment?
            };
            let mut segment = if test_segment.free_bytes() <= buff.len() as u64 {
                seg_id = backend.make_segment()?;
                backend.get_segment(seg_id)?
            } else {
                test_segment
            };

            let (start, length) = segment.write_chunk(&buff)?;
            self.index.insert(id, (seg_id, start, length));

            Some((id, false))
        }
    }

    #[cfg_attr(feature = "profile", flame)]
    /// Writes a chunk to the repo
    ///
    /// Uses all defaults
    ///
    /// Will return None if writing the chunk fails.
    /// Will not write the chunk if it already exists.

    /// Bool in return value will be true if the chunk already existed in the
    /// Repository, and false otherwise
    pub fn write_chunk(&mut self, data: Vec<u8>) -> Option<(Key, bool)> {
        let chunk = Chunk::pack(
            data,
            self.compression,
            self.encryption.new_iv(),
            self.hmac,
            &self.key,
        );

        self.write_raw(chunk)
    }

    #[cfg_attr(feature = "profile", flame)]
    /// Writes a chunk to the repo
    ///
    /// Uses all defaults
    ///
    /// Will return None if writing the chunk fails.
    /// Will not write the chunk if it already exists.
    ///
    /// Manually sets the id of the written chunk.
    /// This should be used carefully, as it has potential to damage the repository.
    ///
    /// Primiarly intended for writing the manifest
    pub fn write_chunk_with_id(&mut self, data: Vec<u8>, id: Key) -> Option<(Key, bool)> {
        let chunk = Chunk::pack_with_id(
            data,
            self.compression,
            self.encryption.new_iv(),
            self.hmac,
            &self.key,
            id,
        );

        self.write_raw(chunk)
    }

    /// Determines if a chunk exists in the index
    pub fn has_chunk(&self, id: Key) -> bool {
        self.index.contains_key(&id)
    }

    #[cfg_attr(feature = "profile", flame)]
    /// Reads a chunk from the repo
    ///
    /// Returns none if reading the chunk fails
    pub fn read_chunk(&self, id: Key) -> Option<Vec<u8>> {
        // First, check if the chunk exists
        if self.has_chunk(id) {
            let (seg_id, start, length) = *self.index.get(&id)?;
            let mut segment = self.backend.get_segment(seg_id)?;
            let chunk_bytes = segment.read_chunk(start, length)?;

            let mut de = Deserializer::new(&chunk_bytes[..]);
            let chunk: Chunk = Deserialize::deserialize(&mut de).unwrap();

            let data = chunk.unpack(&self.key)?;

            Some(data)
        } else {
            None
        }
    }

    /// Provides a count of the number of chunks in the repository
    pub fn count_chunk(&self) -> usize {
        self.index.len()
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        self.commit_index();
    }
}

/// Key for an object in a repository
#[derive(PartialEq, Eq, Copy, Clone, Serialize, Deserialize, Hash, Debug)]
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
        key[..cmp::min(32, input_key.len())]
            .clone_from_slice(&input_key[..cmp::min(32, input_key.len())]);
        Key { key }
    }

    /// Returns an immutable refrence to the key in bytestring form
    pub fn get_key(&self) -> &[u8] {
        &self.key
    }

    /// Verifies equaliy of this key with the first 32 bytes of a slice
    pub fn verfiy(&self, slice: &[u8]) -> bool {
        if slice.len() < self.key.len() {
            false
        } else {
            let mut equal = true;
            for i in 0..self.key.len() {
                if self.key[i] != slice[i] {
                    equal = false;
                }
            }
            equal
        }
    }

    /// Returns the special all-zero key used for the manifest
    pub fn mainfest_key() -> Key {
        Key { key: [0_u8; 32] }
    }
}

/// Chunk Settings
///
/// Encapsulates the Encryption, Compression, and HMAC tags for a chunk
#[derive(Serialize, Deserialize, Clone)]
pub struct ChunkSettings {
    pub compression: Compression,
    pub encryption: Encryption,
    pub hmac: HMAC,
}

/// Data chunk
///
/// Encrypted, compressed object, to be stored in the repository
#[derive(Serialize, Deserialize)]
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
    id: Key,
}

impl Chunk {
    #[cfg_attr(feature = "profile", flame)]
    /// Will Pack the data into a chunk with the given compression and encryption
    pub fn pack(
        data: Vec<u8>,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: &[u8],
    ) -> Chunk {
        let id_mac = hmac.mac(&data, key);
        let compressed_data = compression.compress(data);
        let data = encryption.encrypt(&compressed_data, key);
        let id = Key::new(&id_mac);
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

    #[cfg_attr(feature = "profile", flame)]
    /// Will pack a chunk, but manually setting the id instead of hashing
    ///
    /// This function should be used carefully, as it has potentiall to do major damage to the repository
    pub fn pack_with_id(
        data: Vec<u8>,
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: &[u8],
        id: Key,
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

    #[cfg_attr(feature = "profile", flame)]
    /// Decrypts and decompresses the data in the chunk
    ///
    /// Will return none if either the decompression or the decryption fail
    ///
    /// Will also return none if the HMAC verification fails
    pub fn unpack(&self, key: &[u8]) -> Option<Vec<u8>> {
        if self.hmac.verify(&self.mac, &self.data, key) {
            let decrypted_data = self.encryption.decrypt(&self.data, key)?;
            let decompressed_data = self.compression.decompress(decrypted_data)?;

            Some(decompressed_data)
        } else {
            None
        }
    }

    /// Creates a chunk from a raw bytestring with the given compressor
    /// and encryption algorithim
    pub fn from_bytes(
        data: &[u8],
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        mac: &[u8],
        id: Key,
    ) -> Chunk {
        Chunk {
            data: data.to_vec(),
            compression,
            encryption,
            hmac,
            mac: mac.to_vec(),
            id,
        }
    }

    /// Returns the length of the data bytes
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Determine if this chunk is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Returns a reference to the raw bytes of this chunk
    pub fn get_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Gets the key for this block
    pub fn get_id(&self) -> Key {
        self.id
    }

    #[cfg(test)]
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
    use rand::prelude::*;
    use tempfile::tempdir;

    fn chunk_with_settings(compression: Compression, encryption: Encryption, hmac: HMAC) {
        let data_string =
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";

        let data_bytes = data_string.as_bytes().to_vec();
        println!("Data: \n:{:X?}", data_bytes);

        let mut key: [u8; 32] = [0; 32];
        thread_rng().fill_bytes(&mut key);

        let packed = Chunk::pack(data_bytes, compression, encryption, hmac, &key);

        let output_bytes = packed.unpack(&key);

        assert_eq!(Some(data_string.as_bytes().to_vec()), output_bytes);
    }

    fn get_repo(key: &[u8; 32]) -> Repository {
        let root_dir = tempdir().unwrap();
        let root_path = root_dir.path().display().to_string();

        let backend = Box::new(FileSystem::new_test(&root_path));
        Repository::new(
            backend,
            Compression::ZStd { level: 1 },
            HMAC::Blake2b,
            Encryption::new_aes256ctr(),
            key,
        )
    }

    #[test]
    fn chunk_aes256cbc_zstd6_sha256() {
        let compression = Compression::ZStd { level: 6 };
        let encryption = Encryption::new_aes256cbc();
        let hmac = HMAC::SHA256;
        chunk_with_settings(compression, encryption, hmac);
    }

    #[test]
    fn chunk_aes256cbc_zstd6_blake2b() {
        let compression = Compression::ZStd { level: 6 };
        let encryption = Encryption::new_aes256cbc();
        let hmac = HMAC::Blake2b;
        chunk_with_settings(compression, encryption, hmac);
    }

    #[test]
    fn chunk_aes256ctr_zstd6_blake2b() {
        let compression = Compression::ZStd { level: 6 };
        let encryption = Encryption::new_aes256ctr();
        let hmac = HMAC::Blake2b;
        chunk_with_settings(compression, encryption, hmac);
    }

    #[test]
    fn detect_bad_data() {
        let data_string = "I am but a humble test string";
        let data_bytes = data_string.as_bytes().to_vec();
        let compression = Compression::NoCompression;
        let encryption = Encryption::NoEncryption;
        let hmac = HMAC::SHA256;

        let mut key: [u8; 32] = [0; 32];
        thread_rng().fill_bytes(&mut key);

        let mut packed = Chunk::pack(data_bytes, compression, encryption, hmac, &key);
        packed.break_data(5);

        let result = packed.unpack(&key);

        assert_eq!(result, None);
    }

    #[test]
    fn repository_add_read() {
        let mut key: [u8; 32] = [0; 32];
        thread_rng().fill_bytes(&mut key);

        let size = 7 * 10_u64.pow(3);
        let mut data1 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data1);
        let mut data2 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data2);
        let mut data3 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data3);

        let mut repo = get_repo(&key);

        println!("Adding Chunks");
        let key1 = repo.write_chunk(data1.clone()).unwrap().0;
        let key2 = repo.write_chunk(data2.clone()).unwrap().0;
        let key3 = repo.write_chunk(data3.clone()).unwrap().0;

        println!("Reading Chunks");
        let out1 = repo.read_chunk(key1).unwrap();
        let out2 = repo.read_chunk(key2).unwrap();
        let out3 = repo.read_chunk(key3).unwrap();

        assert_eq!(data1, out1);
        assert_eq!(data2, out2);
        assert_eq!(data3, out3);
    }

    #[test]
    fn repository_add_drop_read() {
        let mut key: [u8; 32] = [0; 32];
        thread_rng().fill_bytes(&mut key);

        let size = 7 * 10_u64.pow(3);
        let mut data1 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data1);
        let mut data2 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data2);
        let mut data3 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data3);

        let root_dir = tempdir().unwrap();
        let root_path = root_dir.path().display().to_string();
        println!("Repo root dir: {}", root_path);

        let backend = Box::new(FileSystem::new_test(&root_path));
        let key1;
        let key2;
        let key3;

        {
            let mut repo = Repository::new(
                backend,
                Compression::ZStd { level: 1 },
                HMAC::SHA256,
                Encryption::new_aes256cbc(),
                &key,
            );

            println!("Adding Chunks");
            key1 = repo.write_chunk(data1.clone()).unwrap().0;
            key2 = repo.write_chunk(data2.clone()).unwrap().0;
            key3 = repo.write_chunk(data3.clone()).unwrap().0;
        }

        let backend = Box::new(FileSystem::new_test(&root_path));

        let repo = Repository::new(
            backend,
            Compression::ZStd { level: 1 },
            HMAC::SHA256,
            Encryption::new_aes256cbc(),
            &key,
        );

        println!("Reading Chunks");
        let out1 = repo.read_chunk(key1).unwrap();
        let out2 = repo.read_chunk(key2).unwrap();
        let out3 = repo.read_chunk(key3).unwrap();

        assert_eq!(data1, out1);
        assert_eq!(data2, out2);
        assert_eq!(data3, out3);
    }

    #[test]
    fn double_add() {
        // Adding the same chunk to the repository twice shouldn't result in
        // two chunks in the repository
        let mut repo = get_repo(&[0_u8; 32]);
        assert_eq!(repo.count_chunk(), 0);
        let data = [1_u8; 8192];

        let (key_1, unique_1) = repo.write_chunk(data.to_vec()).unwrap();
        assert_eq!(unique_1, false);
        assert_eq!(repo.count_chunk(), 1);
        let (key_2, unique_2) = repo.write_chunk(data.to_vec()).unwrap();
        assert_eq!(repo.count_chunk(), 1);
        assert_eq!(unique_2, true);
        assert_eq!(key_1, key_2);
    }

}
