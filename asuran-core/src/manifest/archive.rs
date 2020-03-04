use crate::manifest::listing::Listing;
use crate::repository::ChunkID;

use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;

/// A pointer to a `Chunk`, annotated with information on what part of the object it
/// makes up
#[derive(Serialize, Deserialize, Copy, Clone, Eq, PartialEq, Debug)]
pub struct ChunkLocation {
    pub id: ChunkID,
    pub start: u64,
    pub length: u64,
}

impl PartialOrd for ChunkLocation {
    fn partial_cmp(&self, other: &ChunkLocation) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ChunkLocation {
    fn cmp(&self, other: &ChunkLocation) -> Ordering {
        self.start.cmp(&other.start)
    }
}

/// An Archive, as stored in the repository
#[derive(Serialize, Deserialize)]
pub struct Archive {
    /// The user provided name of the archive
    pub name: String,
    /// The list of objects in this archive, as well as the chunks that make them up
    pub objects: HashMap<String, Vec<ChunkLocation>>,
    /// The namespace this archive is currently viewing
    pub namespace: Vec<String>,
    /// The timestamp of the archive's creation
    pub timestamp: DateTime<FixedOffset>,
    /// The listing of objects in the repository, maintaining their relative structure,
    /// such as the layout of directories and folders.
    pub listing: Listing,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
/// Extent range
///
/// Values are 0 indexed
pub struct Extent {
    pub start: u64,
    pub end: u64,
}
