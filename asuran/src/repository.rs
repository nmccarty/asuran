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
//! chunk basis, with `Encryption::NoEncryption` and `Compression::NoCompression`
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
use thiserror::Error;

pub use crate::repository::backend::{Backend, Index, SegmentDescriptor};
use crate::repository::pipeline::Pipeline;
pub use asuran_core::repository::chunk::{Chunk, ChunkID, ChunkSettings, UnpackedChunk};
pub use asuran_core::repository::compression::Compression;
pub use asuran_core::repository::encryption::Encryption;
pub use asuran_core::repository::hmac::HMAC;
pub use asuran_core::repository::key::{EncryptedKey, Key};

use tracing::{debug, info, instrument, span, trace, Level};

pub mod backend;
pub mod pipeline;

/// An error for all the various things that can go wrong with handling chunks
#[derive(Error, Debug)]
pub enum RepositoryError {
    #[error("Chunk Not in Repository")]
    ChunkNotFound,
    #[error("Chunker Error")]
    ChunkerError(#[from] asuran_core::repository::chunk::ChunkError),
    #[error("Backend Error")]
    BackendError(#[from] backend::BackendError),
}

type Result<T> = std::result::Result<T, RepositoryError>;

/// Provides an interface to the storage-backed key value store
///
/// File access is abstracted behind a swappable backend, all backends should
/// use roughly the same format, but leeway is made for cases such as S3 having
/// a flat directory structure
#[derive(Clone)]
pub struct Repository<T> {
    backend: T,
    /// Default compression for new chunks
    compression: Compression,
    /// Default MAC algorthim for new chunks
    hmac: HMAC,
    /// Default encryption algorthim for new chunks
    encryption: Encryption,
    /// Encryption key for this repo
    key: Key,
    /// Pipeline used for chunking
    pipeline: Pipeline,
}

impl<T: Backend + 'static> Repository<T> {
    /// Creates a new repository with the specificed backend and defaults
    #[instrument(skip(key))]
    pub fn new(
        backend: T,
        compression: Compression,
        hmac: HMAC,
        encryption: Encryption,
        key: Key,
    ) -> Repository<T> {
        info!("Creating a repository with backend {:?}", backend);
        let pipeline = Pipeline::new();
        Repository {
            backend,
            compression,
            hmac,
            encryption,
            key,
            pipeline,
        }
    }

    /// Creates a new repository, accepting a ChunkSettings and a ThreadPool
    #[instrument(skip(key))]
    pub fn with(backend: T, settings: ChunkSettings, key: Key) -> Repository<T> {
        info!(
            "Creating a repository with backend {:?} and chunk settings {:?}",
            backend, settings
        );
        let pipeline = Pipeline::new();
        Repository {
            backend,
            key,
            pipeline,
            compression: settings.compression,
            hmac: settings.hmac,
            encryption: settings.encryption,
        }
    }

    /// Commits the index to storage
    ///
    /// This should be called every time an archive or manifest is written, at
    /// the very least
    #[instrument(skip(self))]
    pub async fn commit_index(&self) {
        debug!("Commiting Index");
        self.backend
            .get_index()
            .commit_index()
            .await
            .expect("Unable to commit index");
    }

    /// Writes a chunk directly to the repository
    ///
    /// Will return (Chunk_Id, Already_Present)
    ///
    /// Already_Present will be true if the chunk already exists in the
    /// repository.
    pub async fn write_raw(&mut self, chunk: Chunk) -> Result<(ChunkID, bool)> {
        let id = chunk.get_id();
        let span = span!(Level::DEBUG, "Writing Chunk", ?id);
        let _guard = span.enter();
        debug!("Writing chunk with id {:?}", id);

        // Check if chunk exists
        if self.has_chunk(id).await && id != ChunkID::manifest_id() {
            trace!("Chunk already existed, doing nothing.");
            Ok((id, true))
        } else {
            trace!("Chunk did not exist, continuning");

            // Get highest segment and check to see if has enough space
            let backend = &mut self.backend;
            let location = backend.write_chunk(chunk, id).await?;

            self.backend
                .get_index()
                .set_chunk(id, location)
                .await?;

            Ok((id, false))
        }
    }

    /// Writes a chunk to the repo
    ///
    /// Uses all defaults
    ///
    /// Will return None if writing the chunk fails.
    /// Will not write the chunk if it already exists.

    /// Bool in return value will be true if the chunk already existed in the
    /// Repository, and false otherwise
    #[instrument(skip(self, data))]
    pub async fn write_chunk(&mut self, data: Vec<u8>) -> Result<(ChunkID, bool)> {
        let (_, chunk) = self
            .pipeline
            .process(
                data,
                self.compression,
                self.encryption,
                self.hmac,
                self.key.clone(),
            )
            .await;
        self.write_raw(chunk).await
    }

    /// Writes an unpacked chunk to the repository using all defaults
    #[instrument(skip(self, data))]
    pub async fn write_unpacked_chunk(&mut self, data: UnpackedChunk) -> Result<(ChunkID, bool)> {
        let id = data.id();
        self.write_chunk_with_id(data.consuming_data(), id).await
    }

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
    #[instrument(skip(self, data))]
    pub async fn write_chunk_with_id(
        &mut self,
        data: Vec<u8>,
        id: ChunkID,
    ) -> Result<(ChunkID, bool)> {
        let chunk = self
            .pipeline
            .process_with_id(
                data,
                id,
                self.compression,
                self.encryption,
                self.hmac,
                self.key.clone(),
            )
            .await;
        self.write_raw(chunk).await
    }

    /// Determines if a chunk exists in the index
    #[instrument(skip(self))]
    pub async fn has_chunk(&self, id: ChunkID) -> bool {
        self.backend.get_index().lookup_chunk(id).await.is_some()
    }

    /// Reads a chunk from the repo
    ///
    /// Returns none if reading the chunk fails
    #[instrument(skip(self))]
    pub async fn read_chunk(&mut self, id: ChunkID) -> Result<Vec<u8>> {
        // First, check if the chunk exists
        if self.has_chunk(id).await {
            let mut index = self.backend.get_index();
            let location = index.lookup_chunk(id).await.unwrap();
            let chunk = self.backend.read_chunk(location).await?;

            let data = chunk.unpack(&self.key)?;

            Ok(data)
        } else {
            Err(RepositoryError::ChunkNotFound)
        }
    }

    /// Provides a count of the number of chunks in the repository
    #[instrument(skip(self))]
    pub async fn count_chunk(&self) -> usize {
        self.backend.get_index().count_chunk().await
    }

    /// Returns the current default chunk settings for this repository
    #[instrument(skip(self))]
    pub fn chunk_settings(&self) -> ChunkSettings {
        ChunkSettings {
            encryption: self.encryption,
            compression: self.compression,
            hmac: self.hmac,
        }
    }

    /// Gets a refrence to the repository's key
    #[instrument(skip(self))]
    pub fn key(&self) -> &Key {
        &self.key
    }

    /// Provides a handle to the backend manifest
    #[instrument(skip(self))]
    pub fn backend_manifest(&self) -> T::Manifest {
        self.backend.get_manifest()
    }

    /// Performs any work that would normally be done in a drop impl, but needs to be done
    /// asyncronsyly.
    ///
    /// Calls into the backend's implementation
    #[instrument(skip(self))]
    pub async fn close(self) {
        self.backend.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::backend::common::sync_backend::BackendHandle;
    use crate::repository::backend::mem::*;
    use rand::prelude::*;

    fn get_repo_mem(key: Key) -> Repository<BackendHandle<Mem>> {
        let settings = ChunkSettings {
            compression: Compression::ZStd { level: 1 },
            hmac: HMAC::Blake2b,
            encryption: Encryption::new_aes256ctr(),
        };
        let backend = Mem::new(settings);
        Repository::with(backend, settings, key)
    }

    #[tokio::test(threaded_scheduler)]
    async fn repository_add_read() {
        let key = Key::random(32);

        let size = 7 * 10_u64.pow(3);
        let mut data1 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data1);
        let mut data2 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data2);
        let mut data3 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data3);

        let mut repo = get_repo_mem(key);
        println!("Adding Chunks");
        let key1 = repo.write_chunk(data1.clone()).await.unwrap().0;
        let key2 = repo.write_chunk(data2.clone()).await.unwrap().0;
        let key3 = repo.write_chunk(data3.clone()).await.unwrap().0;

        println!("Reading Chunks");
        let out1 = repo.read_chunk(key1).await.unwrap();
        let out2 = repo.read_chunk(key2).await.unwrap();
        let out3 = repo.read_chunk(key3).await.unwrap();

        assert_eq!(data1, out1);
        assert_eq!(data2, out2);
        assert_eq!(data3, out3);
    }

    #[tokio::test(threaded_scheduler)]
    async fn double_add() {
        // Adding the same chunk to the repository twice shouldn't result in
        // two chunks in the repository
        let mut repo = get_repo_mem(Key::random(32));
        assert_eq!(repo.count_chunk().await, 0);
        let data = [1_u8; 8192];

        let (key_1, unique_1) = repo.write_chunk(data.to_vec()).await.unwrap();
        assert_eq!(unique_1, false);
        assert_eq!(repo.count_chunk().await, 1);
        let (key_2, unique_2) = repo.write_chunk(data.to_vec()).await.unwrap();
        assert_eq!(repo.count_chunk().await, 1);
        assert_eq!(unique_2, true);
        assert_eq!(key_1, key_2);
        std::mem::drop(repo);
    }
}
