use crate::repository::backend::SegmentDescriptor;
use crate::repository::ChunkID;
use serde::{Deserialize, Serialize};
/// Struct containing the various parts of a transaction
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct IndexTransaction {
    pub chunk_id: ChunkID,
    pub descriptor: SegmentDescriptor,
}
