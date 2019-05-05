pub mod filesystem;

pub trait Segment {
    /// Returns the free bytes in this segment
    fn free_bytes(&self) -> usize;
    /// Reads a chunk from the segment into a bytestring
    ///
    /// Requires the start and end positions for the chunk
    ///
    /// Will return None if the read fails
    fn read_chunk(&self, start: usize, end: usize) -> Option<Vec<u8>>;
    /// Writes a chunk to the segment
    ///
    /// Retuns Some(star,end), or None if writing fails
    fn write_chunk(&self, chunk: &[u8]) -> Option<(usize, usize)>;
}

pub trait Backend {
    /// Gets a particular segment
    ///
    /// Returns None if it does not exist or can not be found
    fn get_segment(&self, id: usize) -> Box<dyn Segment>;
    /// Creates a new segment
    ///
    /// Returns Some(id) with the segement if it can be created
    /// Returns None if creation fails.
    fn make_segment(&self) -> Option<usize>;
    /// Returns the index of the repository
    ///
    /// Indexes are stored as byte strings, intrepreation is up to the caller
    fn get_index(&self) -> Vec<u8>;
    /// Writes a new index to the backend
    ///
    /// Backend should write the new index first, and then delete the old one
    ///
    /// Returns false if the index could not be written.
    fn write_index(&self, index: &[u8], id: usize) -> bool;
}
