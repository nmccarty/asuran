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
pub mod listing;
pub mod target;

use crate::repository::backend::Manifest as BackendManifest;
use crate::repository::{Backend, ChunkSettings, Repository};

use chrono::prelude::*;
use serde::{Deserialize, Serialize};

pub use self::archive::{Archive, StoredArchive};

/// Repository manifest
///
/// This is the root object of the repository, all objects that are active can
/// be reached through the Mainfest.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Manifest<T: Backend> {
    internal_manifest: T::Manifest,
}

impl<T: Backend> Manifest<T> {
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
    pub fn set_chunk_settings(&mut self, settings: ChunkSettings) {
        self.internal_manifest.write_chunk_settings(settings);
    }

    /// Gets the default Chunk Settings for the repository
    pub fn chunk_settings(&self) -> ChunkSettings {
        self.internal_manifest.chunk_settings()
    }

    /// Commits an archive to the manifest, then the manifest to the repository
    ///
    /// Consumes the repository while commiting it.
    ///
    /// # Panics
    ///
    /// Will panic if commiting the archive to the repository fails
    pub fn commit_archive(&mut self, repo: &mut Repository<impl Backend>, archive: Archive) {
        let stored_archive = archive.store(repo);
        self.internal_manifest.write_archive(stored_archive);
        repo.commit_index();
    }

    /// Returns a copy of the list of archives in this repository
    ///
    /// Theses can be converted into full archives with StoredArchive::load
    pub fn archives(&self) -> Vec<StoredArchive> {
        self.internal_manifest.archive_iterator().collect()
    }

    /// Provides the timestamp of the manifest's last modification
    pub fn timestamp(&self) -> DateTime<FixedOffset> {
        self.internal_manifest.last_modification()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::*;

    #[test]
    fn chunk_settings_sanity() {
        let settings = ChunkSettings {
            encryption: Encryption::NoEncryption,
            compression: Compression::NoCompression,
            hmac: HMAC::Blake2b,
        };

        let backend = crate::repository::backend::mem::Mem::new(settings);
        let key = Key::random(32);
        let repo = Repository::new(
            backend,
            settings.compression,
            settings.hmac,
            settings.encryption,
            key,
        );
        let mut manifest = Manifest::load(&repo);

        manifest.set_chunk_settings(settings);
        let new_settings = manifest.chunk_settings();

        assert_eq!(settings, new_settings);
    }
}
