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
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub use self::chunk::{Chunk, ChunkID, ChunkSettings, UnpackedChunk};
pub use crate::repository::backend::filesystem::FileSystem;
pub use crate::repository::backend::Backend;
pub use crate::repository::compression::Compression;
pub use crate::repository::encryption::Encryption;
pub use crate::repository::hmac::HMAC;
pub use crate::repository::key::{EncryptedKey, Key};

#[cfg(feature = "profile")]
use flamer::*;

pub mod backend;
pub mod chunk;
pub mod compression;
pub mod encryption;
pub mod hmac;
pub mod key;

/// Provides an interface to the storage-backed key value store
///
/// File access is abstracted behind a swappable backend, all backends should
/// use roughly the same format, but leeway is made for cases such as S3 having
/// a flat directory structure
#[derive(Clone)]
pub struct Repository<T: Backend> {
    backend: T,
    index: Arc<RwLock<HashMap<ChunkID, (u64, u64, u64)>>>,
    /// Default compression for new chunks
    compression: Compression,
    /// Default MAC algorthim for new chunks
    hmac: HMAC,
    /// Default encryption algorthim for new chunks
    encryption: Encryption,
    /// Encryption key for this repo
    key: Key,
}

impl<T: Backend> Repository<T> {
    /// Creates a new repository with the specificed backend and defaults
    pub fn new(
        backend: T,
        compression: Compression,
        hmac: HMAC,
        encryption: Encryption,
        key: Key,
    ) -> Repository<T> {
        // Check for index, create a new one if it doesnt exist
        let index_vec = backend.get_index();
        let index = if index_vec.is_empty() {
            Arc::new(RwLock::new(HashMap::new()))
        } else {
            let mut de = Deserializer::new(index_vec.as_slice());
            Arc::new(RwLock::new(
                Deserialize::deserialize(&mut de).expect("Unable to parse index"),
            ))
        };

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
    pub fn write_raw(&mut self, chunk: Chunk) -> Option<(ChunkID, bool)> {
        let id = chunk.get_id();

        // Check if chunk exists
        if self.has_chunk(id) && id != ChunkID::manifest_id() {
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
            self.index
                .write()
                .unwrap()
                .insert(id, (seg_id, start, length));

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
    pub fn write_chunk(&mut self, data: Vec<u8>) -> Option<(ChunkID, bool)> {
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
    pub fn write_chunk_with_id(&mut self, data: Vec<u8>, id: ChunkID) -> Option<(ChunkID, bool)> {
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
    pub fn has_chunk(&self, id: ChunkID) -> bool {
        self.index.read().unwrap().contains_key(&id)
    }

    #[cfg_attr(feature = "profile", flame)]
    /// Reads a chunk from the repo
    ///
    /// Returns none if reading the chunk fails
    pub fn read_chunk(&self, id: ChunkID) -> Option<Vec<u8>> {
        // First, check if the chunk exists
        if self.has_chunk(id) {
            let index = self.index.read().unwrap();
            let (seg_id, start, length) = *index.get(&id)?;
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
        self.index.read().unwrap().len()
    }

    /// Returns the current default chunk settings for this repository
    pub fn chunk_settings(&self) -> ChunkSettings {
        ChunkSettings {
            encryption: self.encryption,
            compression: self.compression,
            hmac: self.hmac,
        }
    }

    /// Gets a refrence to the repository's key
    pub fn key(&self) -> &Key {
        &self.key
    }
}

impl<T: Backend> Drop for Repository<T> {
    fn drop(&mut self) {
        self.commit_index();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::prelude::*;
    use tempfile::{tempdir, TempDir};

    fn get_repo(key: Key) -> (Repository<FileSystem>, TempDir) {
        let root_dir = tempdir().unwrap();
        let root_path = root_dir.path().display().to_string();

        let backend = FileSystem::new_test(&root_path);
        (
            Repository::new(
                backend,
                Compression::ZStd { level: 1 },
                HMAC::Blake2b,
                Encryption::new_aes256ctr(),
                key,
            ),
            root_dir,
        )
    }

    #[test]
    fn repository_add_read() {
        let key = Key::random(32);

        let size = 7 * 10_u64.pow(3);
        let mut data1 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data1);
        let mut data2 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data2);
        let mut data3 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data3);

        let (mut repo, root_dir) = get_repo(key);

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
        std::mem::drop(repo);
    }

    #[test]
    fn repository_add_drop_read() {
        let key = Key::random(32);

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

        let backend = FileSystem::new_test(&root_path);
        let key1;
        let key2;
        let key3;

        {
            let mut repo = Repository::new(
                backend,
                Compression::ZStd { level: 1 },
                HMAC::SHA256,
                Encryption::new_aes256cbc(),
                key.clone(),
            );

            println!("Adding Chunks");
            key1 = repo.write_chunk(data1.clone()).unwrap().0;
            key2 = repo.write_chunk(data2.clone()).unwrap().0;
            key3 = repo.write_chunk(data3.clone()).unwrap().0;
        }

        let backend = FileSystem::new_test(&root_path);

        let repo = Repository::new(
            backend,
            Compression::ZStd { level: 1 },
            HMAC::SHA256,
            Encryption::new_aes256cbc(),
            key.clone(),
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
        let (mut repo, root_dir) = get_repo(Key::random(32));
        assert_eq!(repo.count_chunk(), 0);
        let data = [1_u8; 8192];

        let (key_1, unique_1) = repo.write_chunk(data.to_vec()).unwrap();
        assert_eq!(unique_1, false);
        assert_eq!(repo.count_chunk(), 1);
        let (key_2, unique_2) = repo.write_chunk(data.to_vec()).unwrap();
        assert_eq!(repo.count_chunk(), 1);
        assert_eq!(unique_2, true);
        assert_eq!(key_1, key_2);
        std::mem::drop(repo);
    }

    #[test]
    fn repo_send_sync() {}
    #[test]
    fn immediate_drop() {
        // This was resulting in a SIG
        let key = Key::random(32);
        let (mut repo, root_dir) = get_repo(key);
        repo.commit_index();
        println!("Index comiitted!");
        std::mem::drop(repo);
        assert!(true);
    }
}
