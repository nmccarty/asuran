//! The backend provides abstract IO access to the real location of the data in
//! the repository.
use crate::manifest::StoredArchive;
use crate::repository::ChunkID;
use crate::repository::ChunkSettings;
use crate::repository::EncryptedKey;
use anyhow::Result;
use async_trait::async_trait;
use chrono::prelude::*;
use serde::{Deserialize, Serialize};

pub mod common;
pub mod mem;
pub mod multifile;

/// Describes the segment id and location there in of a chunk
///
/// This does not store the lenght, as segments are responsible for storing chunks in a format
/// that does not require prior knowlege of the chunk lenght.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentDescriptor {
    pub segment_id: u64,
    pub start: u64,
}

/// Manifest trait
///
/// Keeps track of which archives are in the repository.
///
/// All writing methods should commit to hard storage prior to returning
#[async_trait]
pub trait Manifest: Send + Sync + Clone + std::fmt::Debug {
    type Iterator: Iterator<Item = StoredArchive>;
    /// Timestamp of the last modification
    async fn last_modification(&self) -> DateTime<FixedOffset>;
    /// Returns the default settings for new chunks in this repository
    async fn chunk_settings(&self) -> ChunkSettings;
    /// Returns an iterator over the list of archives in this repository, in reverse chronological
    /// order (newest first).
    async fn archive_iterator(&self) -> Self::Iterator;

    /// Sets the chunk settings in the repository
    async fn write_chunk_settings(&mut self, settings: ChunkSettings);
    /// Adds an archive to the manifest
    async fn write_archive(&mut self, archive: StoredArchive);
    /// Updates the timestamp without performing any other operations
    async fn touch(&mut self);
}

/// Index Trait
///
/// Keeps track of where chunks are in the backend
#[async_trait]
pub trait Index: Send + Sync + Clone + std::fmt::Debug {
    /// Provides the location of a chunk in the repository
    async fn lookup_chunk(&mut self, id: ChunkID) -> Option<SegmentDescriptor>;
    /// Sets the location of a chunk in the repository
    async fn set_chunk(&mut self, id: ChunkID, location: SegmentDescriptor) -> Result<()>;
    /// Commits the index
    async fn commit_index(&mut self) -> Result<()>;
    /// Returns the total number of chunks in the index
    async fn count_chunk(&mut self) -> usize;
}

/// Repository backend
///
/// The backend handles the heavy lifiting of the IO, abstracting the repository
/// struct itself away from the details of the system used to store the repository.
///
/// Cloning a backend should result in a new view over the same storage, and clones
/// should play nice with multithreaded access.
#[async_trait]
pub trait Backend: 'static + Send + Sync + Clone + std::fmt::Debug {
    type Manifest: Manifest + 'static;
    type Index: Index + 'static;
    /// Returns a view of the index of the repository
    fn get_index(&self) -> Self::Index;
    /// Writes the specified encrypted key to the backend
    ///
    /// Returns Err if the key could not be written
    async fn write_key(&self, key: &EncryptedKey) -> Result<()>;
    /// Attempts to read the encrypted key from the backend.
    async fn read_key(&self) -> Result<EncryptedKey>;
    /// Returns a view of this respository's manifest
    fn get_manifest(&self) -> Self::Manifest;
    /// Starts reading a chunk from the backend
    ///
    /// The chunk will be written to the oneshot once reading is complete
    async fn read_chunk(&self, location: SegmentDescriptor) -> Result<Vec<u8>>;
    /// Starts writing a chunk to the backend
    ///
    /// A segment descriptor describing it will be written to oneshot once reading is complete
    ///
    /// This must be passed owned data because it will be sent into a task, so the caller has no control over drop time
    async fn write_chunk(&self, chunk: Vec<u8>, id: ChunkID) -> Result<SegmentDescriptor>;
}

#[derive(Copy, PartialEq, Eq, Clone, Serialize, Deserialize, Debug)]
pub enum TransactionType {
    Insert,
    Delete,
}
