//! This module contains syncronous versions of the backend trait, as well as an abstraction for
//! implementing the main, async traits through holding the syncronous version in a task.
//!
//! This trait is not meant to be consumed directly by a user of this library.
//!
//! Implementors of this trait are required to be send (as the operations are handled on an async task),
//! however, they are not required to be sync.
//!
//! Addtionally, as only one direct consumer of these traits is expected to exist, the implementors are
//! not required to be `Clone`.
//!
//! Methods in this module are intentionally left undocumented, as they are indented to be syncronus
//! versions of their async equivlants in the main Backend traits.
use crate::manifest::StoredArchive;
use crate::repository::backend::{BackendError, Result, SegmentDescriptor};
use crate::repository::{ChunkID, ChunkSettings, EncryptedKey};

use chrono::prelude::*;
use serde::{Deserialize, Serialize};

pub trait SyncManifest: Send + std::fmt::Debug {
    type Iterator: Iterator<Item = StoredArchive>;
    fn last_modification(&mut self) -> Result<DateTime<FixedOffset>>;
    fn chunk_settings(&mut self) -> ChunkSettings;
    fn archive_iterator(&mut self) -> Self::Iterator;
    fn write_chunk_settings(&mut self, settings: ChunkSettings) -> Result<()>;
    fn write_archive(&mut self, archive: StoredArchive) -> Result<()>;
    fn touch(&mut self) -> Result<()>;
}

pub trait SyncIndex: Send + std::fmt::Debug {
    fn lookup_chunk(&mut self, id: ChunkID) -> Option<SegmentDescriptor>;
    fn set_chunk(&mut self, id: ChunkID, location: SegmentDescriptor) -> Result<()>;
    fn commit_index(&mut self) -> Result<()>;
    fn chunk_count(&mut self) -> usize;
}

/// Note: In this version of the trait, the get index and get archive methods return mutable references,
/// instead of owned values. As this version of the trait is intrinsically single threaded, implementers
/// are expected to own a single instance of their Index and Manifest impls, and the reference will
/// never leak outside of their container task.
///
/// Also note, that we do not have the close method, as the wrapper type will handle that for us.
pub trait Backend: 'static + Send + std::fmt::Debug {
    type SyncManifest: SyncManifest + 'static;
    type SyncIndex: SyncIndex + 'static;
    fn get_index(&mut self) -> &mut Self::SyncIndex;
    fn get_manfiest(&mut self) -> &mut Self::SyncManifest;
    fn write_key(&mut self, key: EncryptedKey) -> Result<()>;
    fn read_key(&mut self) -> Result<EncryptedKey>;
    fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Vec<u8>>;
    fn write_chunk(&mut self, location: SegmentDescriptor) -> Result<SegmentDescriptor>;
}
