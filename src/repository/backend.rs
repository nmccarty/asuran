//! The backend provides abstract IO access to the real location of the data in
//! the repository.
use std::io::Result;
use crate::repository::EncryptedKey;

pub mod filesystem;

/// Segments are abstract blocks of chunks
///
/// Backends are free to arrange chunks within segements any way they wish,
/// as long as they only ever append to existing segments.
///
/// Segement compaction must happen through writing new segments and deleting
/// the old ones
pub trait Segment {
    /// Returns the free bytes in this segment
    fn free_bytes(&self) -> u64;
    /// Reads a chunk from the segment into a bytestring
    ///
    /// Requires the start and end positions for the chunk
    ///
    /// Will return None if the read fails
    fn read_chunk(&mut self, start: u64, length: u64) -> Option<Vec<u8>>;
    /// Writes a chunk to the segment
    ///
    /// Retuns Some(start,length), or None if writing fails
    fn write_chunk(&mut self, chunk: &[u8]) -> Option<(u64, u64)>;
}

/// Repository backend
///
/// The backend handles the heavy lifiting of the IO, abstracting the repository
/// struct itself away from the details of the system used to store the repository.
///
/// Cloning a backend should result in a new view over the same storage, and clones
/// should play nice with multithreaded access.
pub trait Backend: Send + Sync + Clone {
    /// Gets a particular segment
    ///
    /// Returns None if it does not exist or can not be found
    fn get_segment(&self, id: u64) -> Option<Box<dyn Segment>>;
    /// Returns the id of the higest segment
    fn highest_segment(&self) -> u64;
    /// Creates a new segment
    ///
    /// Returns Some(id) with the segement if it can be created
    /// Returns None if creation fails.
    fn make_segment(&self) -> Option<u64>;
    /// Returns the index of the repository
    ///
    /// Indexes are stored as byte strings, intrepreation is up to the caller
    fn get_index(&self) -> Vec<u8>;
    /// Writes a new index to the backend
    ///
    /// Backend should write the new index first, and then delete the old one
    ///
    /// Returns Err if the index could not be written.
    fn write_index(&self, index: &[u8]) -> Result<()>;
    /// Writes the specified encrypted key to the backend
    /// 
    /// Returns Err if the key could not be written
    fn write_key(&self, key: &EncryptedKey) -> Result<()>;
    /// Attempts to read the encrypted key from the backend. 
    fn read_key(&self) -> Option<EncryptedKey>;
}
