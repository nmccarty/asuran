//! This module provides the root object of the object graph of a repository.
//!
//! The manifest contains a list of all the archives in the repository, as well
//! default chunk settings, and a time stamp for preventing replay attacks.
//!
//! All operations on a manifest require a reference to the repository for context.
//! The repository is not encapsulated in the manifest because the manifest needs
//! to be triviallly serializeable and deserilazeable.
pub mod archive;
pub mod target;

use crate::repository::{ChunkSettings, Key, Repository};

use chrono::prelude::*;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};

pub use self::archive::{Archive, StoredArchive};

/// Repository manifest
///
/// Has special all zero key
///
/// This is the root object of the repository, all objects that are active can
/// be reached through the Mainfest.
#[derive(Serialize, Deserialize, Clone)]
pub struct Manifest {
    /// Time stamp the manifiest was commited at. This is updated every time commit() is called
    ///
    /// This value is primarly for preventing replay attacks
    timestamp: DateTime<FixedOffset>,
    /// Default settings for new chunks in this repository
    chunk_settings: ChunkSettings,
    /// List of archives in the repository,
    archives: Vec<StoredArchive>,
}

impl Manifest {
    /// Creates a new empty manifest with the given chunk settings
    pub fn empty_manifest(settings: ChunkSettings) -> Manifest {
        Manifest {
            timestamp: Local::now().with_timezone(Local::now().offset()),
            chunk_settings: settings,
            archives: Vec::new(),
        }
    }

    /// Commits the manifest to the repository
    ///
    /// Will overwrite existing manifest. Will additionally update the
    /// timestamp of the mainfest. Commits the index of the repository after it is done
    ///
    /// # Panics
    ///  
    /// Will panic if the write to the repository fails
    pub fn commit(&mut self, repo: &mut Repository) {
        self.timestamp = Local::now().with_timezone(Local::now().offset());

        let mut bytes = Vec::<u8>::new();
        self.serialize(&mut Serializer::new(&mut bytes))
            .expect("Unable to serialize manifest.");

        repo.write_chunk_with_id(&bytes, Key::mainfest_key())
            .expect("Unable to write manifest");

        repo.commit_index();
    }

    /// Loads the manifest from the repository
    ///
    /// # Panics
    ///
    /// Will panic if loading the manifest fails
    pub fn load(repo: Repository) -> Manifest {
        let bytes = repo
            .read_chunk(Key::mainfest_key())
            .expect("Unable to read manifest from repo");

        let mut de = Deserializer::new(&bytes[..]);
        let manifest: Manifest =
            Deserialize::deserialize(&mut de).expect("Unable to deserialze Manifest.");

        manifest
    }

    /// Commits an archive to the manifest, then the manifest to the repository
    ///
    /// Consumes the repository while commiting it.
    ///
    /// # Panics
    ///
    /// Will panic if commiting the archive to the repository fails
    pub fn commit_archive(&mut self, repo: &mut Repository, archive: Archive) {
        let stored_archive = archive.store(repo);
        self.archives.push(stored_archive);

        self.commit(repo);
    }

    /// Returns a copy of the list of archives in this repository
    ///
    /// Theses can be converted into full archives with StoredArchive::load
    pub fn archives(&self) -> Vec<StoredArchive> {
        self.archives.clone()
    }
}
