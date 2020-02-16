use crate::repository::ChunkID;

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

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
