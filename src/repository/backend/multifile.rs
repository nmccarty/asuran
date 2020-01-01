#![allow(unused_variables)]
use crate::repository::backend::*;

use anyhow::Result;
use async_trait::async_trait;
use futures::channel::oneshot;

#[derive(Debug, Clone)]
struct MFManifest {}

#[async_trait]
impl Manifest for MFManifest {
    type Iterator = std::iter::Empty<StoredArchive>;
    /// Timestamp of the last modification
    async fn last_modification(&self) -> DateTime<FixedOffset> {
        todo!()
    }
    /// Returns the default settings for new chunks in this repository
    async fn chunk_settings(&self) -> ChunkSettings {
        todo!()
    }
    /// Returns an iterator over the list of archives in this repository, in reverse chronological
    /// order (newest first).
    async fn archive_iterator(&self) -> Self::Iterator {
        todo!()
    }

    /// Sets the chunk settings in the repository
    async fn write_chunk_settings(&mut self, settings: ChunkSettings) {
        todo!()
    }
    /// Adds an archive to the manifest
    async fn write_archive(&mut self, archive: StoredArchive) {
        todo!()
    }
    /// Updates the timestamp without performing any other operations
    async fn touch(&mut self) {
        todo!()
    }
}

#[derive(Debug, Clone)]
struct MFIndex {}

#[async_trait]
impl Index for MFIndex {
    /// Provides the location of a chunk in the repository
    async fn lookup_chunk(&self, id: ChunkID) -> Option<SegmentDescriptor> {
        todo!()
    }
    /// Sets the location of a chunk in the repository
    async fn set_chunk(&self, id: ChunkID, location: SegmentDescriptor) -> Result<()> {
        todo!()
    }
    /// Commits the index
    async fn commit_index(&self) -> Result<()> {
        todo!()
    }
    /// Returns the total number of chunks in the index
    async fn count_chunk(&self) -> usize {
        todo!()
    }
}

#[derive(Debug, Clone)]
struct MultiFile {}

#[async_trait]
impl Backend for MultiFile {
    type Manifest = MFManifest;
    type Index = MFIndex;

    /// Clones the internal MFManifest
    fn get_index(&self) -> Self::Index {
        todo!();
    }
    /// Clones the internal MFIndex
    fn get_manifest(&self) -> Self::Manifest {
        todo!();
    }
    /// Locks the keyfile and writes the key
    ///
    /// Will return Err if writing the key fails
    async fn write_key(&self, key: &EncryptedKey) -> Result<()> {
        todo!();
    }
    /// Attempts to read the key from the repository
    ///
    /// Returns Err if the key doesn't exist or of another error occurs
    async fn read_key(&self) -> Result<EncryptedKey> {
        todo!();
    }

    /// Starts reading a chunk, and returns a oneshot recieve with the result of that process
    async fn read_chunk(&self, location: SegmentDescriptor) -> oneshot::Receiver<Result<Vec<u8>>> {
        todo!();
    }

    /// Starts writing a chunk, and returns a oneshot reciever with the result of that process
    async fn write_chunk(
        &self,
        chunk: Vec<u8>,
        id: ChunkID,
    ) -> oneshot::Receiver<Result<SegmentDescriptor>> {
        todo!();
    }
}
