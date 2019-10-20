//! A slicer cuts data into chunks based on some predefined method
//!
//! Most typical is content defined slicing, but format specific methods are also quite useful

pub mod fastcdc;

use crate::repository::chunk::*;
use crate::repository::Key;
use std::boxed::Box;
use std::io::Read;

/// Describes something that can slice objects in to chunks in a defined, repeatable manner
///
/// Must store state (including the reader) internally
///
/// Slicers must meet three properites:
/// 1.) Data must be split into one or more chunks
/// 2.) Data must be identical after as simple reconstruction by concatenation
/// 3.) The same data and settings must produce the same slices every time
pub trait Slicer: Sized {
    /// Inserts a reader into the Slicer
    ///
    /// Should clear state and drop previous reader
    fn add_reader(&mut self, reader: Box<dyn Read>);
    /// Returns the next slice of the data, updating the internal state
    fn take_slice(&mut self) -> Option<Vec<u8>>;
    /// Returns a slicer with the same settings but not sharing any internal state
    fn copy_settings(&self) -> Self;
    /// Creates a ChunkIterator out of the slicer and its loaded data
    fn into_chunk_iter(self, settings: ChunkSettings, key: Key) -> ChunkIterator<Self> {
        ChunkIterator {
            slicer: self,
            settings,
            key,
        }
    }
}

pub struct ChunkIterator<T> {
    slicer: T,
    settings: ChunkSettings,
    key: Key,
}

impl<T> Iterator for ChunkIterator<T>
where
    T: Slicer,
{
    type Item = UnpackedChunk;
    fn next(&mut self) -> Option<UnpackedChunk> {
        let slice = self.slicer.take_slice()?;
        Some(UnpackedChunk::new(slice, &self.settings, &self.key))
    }
}
