use crate::repository::backend::SegmentDescriptor;
use crate::repository::ChunkID;

use serde::{Deserialize, Serialize};

/// Struct containing the various parts of a transaction
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct IndexTransaction {
    /// ID of the `Chunk` this transaction refers to
    pub chunk_id: ChunkID,
    /// The location of this `Chunk` on disk
    pub descriptor: SegmentDescriptor,
}
