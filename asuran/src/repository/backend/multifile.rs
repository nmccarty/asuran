#![allow(unused_variables)]
use crate::repository::backend::common::files::*;
use crate::repository::backend::*;
use crate::repository::{ChunkSettings, Key};

use super::{BackendError, Result};
use async_trait::async_trait;
use rmp_serde as rmps;
use std::fs::{create_dir_all, remove_file, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

pub mod index;
pub mod manifest;
pub mod segment;

#[derive(Debug, Clone)]
pub struct MultiFile {
    index_handle: index::Index,
    manifest_handle: manifest::Manifest,
    segment_handle: segment::SegmentHandler,
    path: PathBuf,
    /// Connection uuid, used for read locks.
    uuid: Uuid,
    /// Path to readlock for this connection, must be deleted on close
    read_lock_path: Arc<PathBuf>,
}

impl MultiFile {
    /// Opens a new `MultiFile` backend with default settings
    ///
    /// Subject to change in the near future
    ///
    /// # Errors
    ///
    /// Will error if creating or locking any of the index or manifest files
    /// fails (such as if the user does not have permissions for that
    /// directory), or if any other I/O error occurs
    pub async fn open_defaults(
        path: impl AsRef<Path>,
        chunk_settings: Option<ChunkSettings>,
        key: &Key,
    ) -> Result<MultiFile> {
        // First, check to see if the global lock exists, and return an error early if it does
        let global_lock_path = path.as_ref().join("lock");
        if Path::exists(&global_lock_path) {
            return Err(BackendError::RepositoryGloballyLocked(format!(
                "Global lock for this repository already exists at: {:?}",
                global_lock_path
            )));
        }
        // Generate a uuid
        let uuid = Uuid::new_v4();
        let size_limit = 2_000_000_000;
        let segments_per_directory = 100;
        // Open up an index connection
        let index_handle = index::Index::open(&path)?;
        // Open up a manifest connection
        let mut manifest_handle = manifest::Manifest::open(&path, chunk_settings, key)?;
        let chunk_settings = if let Some(chunk_settings) = chunk_settings {
            chunk_settings
        } else {
            manifest_handle.chunk_settings().await
        };
        // Open up a segment handler connection
        let segment_handle = segment::SegmentHandler::open(
            &path,
            size_limit,
            segments_per_directory,
            chunk_settings,
            key.clone(),
        )?;
        // Make sure the readlocks directory exists
        create_dir_all(path.as_ref().join("readlocks"))?;
        // generate a path to our readlock
        let read_lock_path = path
            .as_ref()
            .join("readlocks")
            .join(uuid.to_simple().to_string());
        // Create the read_lock file
        OpenOptions::new()
            .create(true)
            .write(true)
            .open(&read_lock_path)
            .unwrap();

        let path = path.as_ref().to_path_buf();
        Ok(MultiFile {
            index_handle,
            manifest_handle,
            segment_handle,
            path,
            uuid,
            read_lock_path: Arc::new(read_lock_path),
        })
    }

    /// Reads the encrypted key off the disk
    ///
    /// Does not require that the repository be opened first
    ///
    /// Note: this path is the repository root path, not the key path
    ///
    /// # Errors
    ///
    /// Will error if the key is corrupted or deserialization otherwise fails
    pub fn read_key(path: impl AsRef<Path>) -> Result<EncryptedKey> {
        let key_path = path.as_ref().join("key");
        let file = File::open(&key_path)?;
        Ok(rmps::decode::from_read(&file)?)
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
        let mut file =
            LockedFile::open_read_write(&key_path)?.ok_or(BackendError::FileLockError)?;
        Ok(rmps::encode::write(&mut file, key)?)
    }
    /// Attempts to read the key from the repository
    ///
    /// Returns Err if the key doesn't exist or of another error occurs
    async fn read_key(&self) -> Result<EncryptedKey> {
        let key_path = self.path.join("key");
        let file = File::open(&key_path)?;
        Ok(rmps::decode::from_read(&file)?)
    }

    /// Starts reading a chunk, and returns a oneshot recieve with the result of that process
    async fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        self.segment_handle.read_chunk(location).await
    }

    /// Starts writing a chunk, and returns a oneshot reciever with the result of that process
    async fn write_chunk(&mut self, chunk: Chunk) -> Result<SegmentDescriptor> {
        self.segment_handle.write_chunk(chunk).await
    }

    /// Closes out the index, segment handler, and manifest cleanly, making sure all operations are
    /// completed and all drop impls from inside the tasks are called
    async fn close(&mut self) {
        self.index_handle.close().await;
        self.manifest_handle.close().await;
        self.segment_handle.close().await;
        // Check if the read_lock_file exists and delete it
        if self.read_lock_path.exists() {
            // FIXME: We ignore this error for now, as this method does not currently return a
            // result
            let _ = remove_file(self.read_lock_path.as_ref());
        }
    }

    fn get_object_handle(&self) -> BackendObject {
        backend_to_object(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::Encryption;
    use tempfile::{tempdir, TempDir};

    // Utility function, sets up a tempdir and opens a MultiFile Backend
    async fn setup(key: &Key) -> (TempDir, MultiFile) {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().to_path_buf();
        let mf = MultiFile::open_defaults(path, Some(ChunkSettings::lightweight()), key)
            .await
            .unwrap();
        (tempdir, mf)
    }

    #[tokio::test]
    async fn key_store_load() {
        let key = Key::random(32);
        let (tempdir, mut mf) = setup(&key).await;
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

    // Test to make sure that attempting to open a repository respects an existing global lock
    #[tokio::test]
    async fn repository_global_lock() {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().to_path_buf();
        let key = Key::random(32);
        // Create the lock
        OpenOptions::new()
            .create(true)
            .write(true)
            .open(path.join("lock"))
            .unwrap();
        // Attempt to open the backend
        let mf = MultiFile::open_defaults(path, Some(ChunkSettings::lightweight()), &key).await;
        // This should error
        assert!(mf.is_err());
        // It should also, specifically, be a RepositoryGloballyLocked
        assert!(matches!(mf, Err(BackendError::RepositoryGloballyLocked(_))));
    }

    // Tests to make sure that readlocks are created and destroyed properly
    #[tokio::test]
    async fn read_lock_create_destroy() {
        let key = Key::random(32);
        let (tempdir, mut mf) = setup(&key).await;
        let lock_path: Arc<PathBuf> = mf.read_lock_path.clone();
        // the connection is open, assert that the lock exists
        assert!(lock_path.exists());
        // Close the connection
        mf.close().await;
        std::mem::drop(mf);
        // The connection is now closed, assert that the lock does not exist
        assert!(!lock_path.exists());
    }
}
