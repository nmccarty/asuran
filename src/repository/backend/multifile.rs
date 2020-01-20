#![allow(unused_variables)]
use crate::repository::backend::common::files::*;
use crate::repository::backend::*;
use crate::repository::{ChunkSettings, Key};

use anyhow::{Context, Result};
use async_trait::async_trait;
use rmp_serde as rmps;
use std::fs::File;
use std::path::{Path, PathBuf};

pub mod index;
pub mod manifest;
pub mod segment;

#[derive(Debug, Clone)]
pub struct MultiFile {
    index_handle: index::Index,
    manifest_handle: manifest::Manifest,
    segment_handle: segment::SegmentHandler,
    path: PathBuf,
}

impl MultiFile {
    /// Opens a new MultiFile backend with default settings
    ///
    /// Subject to change in the near future
    pub fn open_defaults(
        path: impl AsRef<Path>,
        chunk_settings: Option<ChunkSettings>,
        key: &Key,
    ) -> Result<MultiFile> {
        let size_limit = 500_000_000;
        let segments_per_directory = 100;
        let index_handle = index::Index::open(&path).context("Failure opening index")?;
        let manifest_handle = manifest::Manifest::open(&path, chunk_settings, key)
            .context("Failure opening manifest")?;
        let segment_handle =
            segment::SegmentHandler::open(&path, size_limit, segments_per_directory)
                .context("Failure opening segment handler")?;
        let path = path.as_ref().to_path_buf();
        Ok(MultiFile {
            index_handle,
            manifest_handle,
            segment_handle,
            path,
        })
    }

    /// Reads the encrypted key off the disk
    ///
    /// Does not require that the repository be opened first
    ///
    /// Note: this path is the repository root path, not the key path
    pub fn read_key(path: impl AsRef<Path>) -> Result<EncryptedKey> {
        let key_path = path.as_ref().join("key");
        let file = File::open(&key_path)?;
        rmps::decode::from_read(&file).context("Unable to deserialize key")
    }
}

#[async_trait]
impl Backend for MultiFile {
    type Manifest = manifest::Manifest;
    type Index = index::Index;

    /// Clones the internal MFManifest
    fn get_index(&self) -> Self::Index {
        self.index_handle.clone()
    }
    /// Clones the internal MFIndex
    fn get_manifest(&self) -> Self::Manifest {
        self.manifest_handle.clone()
    }
    /// Locks the keyfile and writes the key
    ///
    /// Will return Err if writing the key fails
    async fn write_key(&self, key: &EncryptedKey) -> Result<()> {
        let key_path = self.path.join("key");
        let mut file = LockedFile::open_read_write(&key_path)?
            .context("Unable to lock key file for writing")?;
        rmps::encode::write(&mut file, key).context("Unable to serialize key")
    }
    /// Attempts to read the key from the repository
    ///
    /// Returns Err if the key doesn't exist or of another error occurs
    async fn read_key(&self) -> Result<EncryptedKey> {
        let key_path = self.path.join("key");
        let file = File::open(&key_path)?;
        rmps::decode::from_read(&file).context("Unable to deserialize key")
    }

    /// Starts reading a chunk, and returns a oneshot recieve with the result of that process
    async fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Vec<u8>> {
        self.segment_handle.read_chunk(location).await
    }

    /// Starts writing a chunk, and returns a oneshot reciever with the result of that process
    async fn write_chunk(&mut self, chunk: Vec<u8>, id: ChunkID) -> Result<SegmentDescriptor> {
        self.segment_handle.write_chunk(chunk, id).await
    }

    /// Closes out the index, segment handler, and manifest cleanly, making sure all operations are
    /// completed and all drop impls from inside the tasks are called
    async fn close(mut self) {
        self.index_handle.close().await;
        self.manifest_handle.close().await;
        self.segment_handle.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::Encryption;
    use tempfile::{tempdir, TempDir};

    // Utility function, sets up a tempdir and opens a MultiFile Backend
    fn setup(key: &Key) -> (TempDir, MultiFile) {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().to_path_buf();
        let mf = MultiFile::open_defaults(path, Some(ChunkSettings::lightweight()), key).unwrap();
        (tempdir, mf)
    }

    #[tokio::test]
    async fn key_store_load() {
        let key = Key::random(32);
        let (tempdir, mf) = setup(&key);
        // Encrypt the key and store it
        let enc_key = EncryptedKey::encrypt(&key, 512, 1, Encryption::new_aes256ctr(), b"");
        mf.write_key(&enc_key).await.expect("Unable to write key");
        // Load the key back out without unloading
        let enc_key = mf
            .read_key()
            .await
            .expect("Unable to read key (before drop)");
        // Decrypt it and verify equality
        let new_key = enc_key
            .decrypt(b"")
            .expect("Unable to decrypt key (before drop)");
        assert_eq!(key, new_key);
        // Drop the backend and try reading it from scratch
        mf.close().await;
        let enc_key = MultiFile::read_key(tempdir.path()).expect("Unable to read key (after drop)");
        let new_key = enc_key
            .decrypt(b"")
            .expect("Unable to decrypt key (after drop)");
        assert_eq!(key, new_key);
    }
}
