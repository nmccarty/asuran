#![allow(unused_variables)]
use crate::repository::backend::*;

use anyhow::Result;
use async_trait::async_trait;

pub mod index;
pub mod manifest;
pub mod segment;

#[derive(Debug, Clone)]
struct MultiFile {}

#[async_trait]
impl Backend for MultiFile {
    type Manifest = manifest::Manifest;
    type Index = index::Index;

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
    async fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Vec<u8>> {
        todo!();
    }

    /// Starts writing a chunk, and returns a oneshot reciever with the result of that process
    async fn write_chunk(&mut self, chunk: Vec<u8>, id: ChunkID) -> Result<SegmentDescriptor> {
        todo!();
    }
}
