//! This module provides the root object of the object graph of a repository.
//!
//! The manifest contains a list of all the archives in the repository, as well
//! default chunk settings, and a time stamp for preventing replay attacks.
//!
//! All operations on a manifest require a reference to the repository for context.
//! The repository is not encapsulated in the manifest because the manifest needs
//! to be triviallly serializeable and deserilazeable.
pub mod archive;
pub mod driver;
pub mod target;

pub use self::archive::{ActiveArchive, StoredArchive};
use crate::repository::backend::Manifest as BackendManifest;
use crate::repository::backend::Result;
use crate::repository::{Backend, BackendClone, ChunkSettings, Repository};

use chrono::prelude::*;
use serde::{Deserialize, Serialize};

/// Repository manifest
///
/// This is the root object of the repository, all objects that are active can
/// be reached through the Mainfest.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Manifest<T: Backend> {
    internal_manifest: T::Manifest,
}

impl<T: BackendClone> Manifest<T> {
    /// Loads the manifest from the repository
    ///
    /// # Panics
    ///
    /// Will panic if loading the manifest fails
    pub fn load(repo: &Repository<T>) -> Manifest<T>
    where
        T: Backend,
    {
        let internal_manifest = repo.backend_manifest();
        Manifest { internal_manifest }
    }

    /// Set the Chunk Settings used by the repository
    pub async fn set_chunk_settings(&mut self, settings: ChunkSettings) -> Result<()> {
        self.internal_manifest.write_chunk_settings(settings).await
    }

    /// Gets the default Chunk Settings for the repository
    pub async fn chunk_settings(&mut self) -> ChunkSettings {
        self.internal_manifest.chunk_settings().await
    }

    /// Commits an archive to the manifest, then the manifest to the repository
    ///
    /// Consumes the repository while commiting it.
    ///
    /// # Panics
    ///
    /// Will panic if commiting the archive to the repository fails
    pub async fn commit_archive(
        &mut self,
        repo: &mut Repository<impl BackendClone>,
        archive: ActiveArchive,
    ) -> Result<()> {
        let stored_archive = archive.store(repo).await;
        self.internal_manifest.write_archive(stored_archive).await?;
        repo.commit_index().await;
        Ok(())
    }

    /// Returns a copy of the list of archives in this repository
    ///
    /// Theses can be converted into full archives with `StoredArchive::load`
    pub async fn archives(&mut self) -> Vec<StoredArchive> {
        self.internal_manifest.archive_iterator().await.collect()
    }

    /// Provides the timestamp of the manifest's last modification
    pub async fn timestamp(&mut self) -> Result<DateTime<FixedOffset>> {
        self.internal_manifest.last_modification().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::*;

    #[tokio::test(threaded_scheduler)]
    async fn chunk_settings_sanity() {
        let settings = ChunkSettings {
            encryption: Encryption::NoEncryption,
            compression: Compression::NoCompression,
            hmac: HMAC::Blake2b,
        };

        let key = Key::random(32);
        let backend = crate::repository::backend::mem::Mem::new(settings, key.clone(), 4);
        let repo = Repository::with(backend, settings, key, 2);
        let mut manifest = Manifest::load(&repo);

        manifest.set_chunk_settings(settings).await.unwrap();
        let new_settings = manifest.chunk_settings().await;

        assert_eq!(settings, new_settings);
    }

    #[tokio::test(threaded_scheduler)]
    async fn new_archive_updates_time() {
        let settings = ChunkSettings::lightweight();
        let key = Key::random(32);
        let backend = crate::repository::backend::mem::Mem::new(settings, key.clone(), 4);
        let repo = Repository::with(backend.clone(), settings, key, 2);

        let mut manifest = Manifest::load(&repo);

        let dummy1 = StoredArchive::dummy_archive();
        backend.get_manifest().write_archive(dummy1).await.unwrap();
        let time1 = manifest.timestamp().await.unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let dummy2 = StoredArchive::dummy_archive();
        backend.get_manifest().write_archive(dummy2).await.unwrap();
        let time2 = manifest.timestamp().await.unwrap();

        assert!(time2 > time1);
    }
}
