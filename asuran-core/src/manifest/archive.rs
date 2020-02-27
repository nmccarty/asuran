use crate::repository::ChunkID;

use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;

/// Location of a chunk in a file/object
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
    pub name: String,
    pub objects: HashMap<String, Vec<ChunkLocation>>,
    pub namespace: Vec<String>,
    pub timestamp: DateTime<FixedOffset>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
/// Extent range
///
/// Values are 0 indexed
pub struct Extent {
    pub start: u64,
    pub end: u64,
}
