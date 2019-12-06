//! A slicer cuts data into chunks based on some predefined method
//!
//! Most typical is content defined slicing, but format specific methods are also quite useful

pub mod buzhash;
pub mod fastcdc;

use crate::repository::chunk::*;
use crate::repository::Key;
use async_std::task::block_on;
use std::io::Read;
use std::marker::PhantomData;

/// Describes something that can slice objects in to chunks in a defined, repeatable manner
///
/// Must store state (including the reader) internally
///
/// Slicers must meet three properites:
/// 1.) Data must be split into one or more chunks
/// 2.) Data must be identical after as simple reconstruction by concatenation
/// 3.) The same data and settings must produce the same slices every time
pub trait Slicer<R: Read>: Sized {
    type Settings: SlicerSettings<R>;
    /// Inserts a reader into the Slicer
    ///
    /// Should clear state and drop previous reader
    fn add_reader(&mut self, reader: R);
    /// Returns the next slice of the data, updating the internal state
    fn take_slice(&mut self) -> Option<Vec<u8>>;
    /// Returns the associated slicer settings
    fn copy_settings(&self) -> Self::Settings;
    /// Creates a ChunkIterator out of the slicer and its loaded data
    fn into_chunk_iter(self, settings: ChunkSettings, key: Key) -> ChunkIterator<R, Self> {
        ChunkIterator {
            slicer: self,
            settings,
            key,
            marker: PhantomData,
        }
    }
}

pub struct ChunkIterator<R: Read, S: Slicer<R>> {
    slicer: S,
    settings: ChunkSettings,
    key: Key,
    marker: PhantomData<R>,
}

impl<R: Read, S: Slicer<R>> Iterator for ChunkIterator<R, S> {
    type Item = UnpackedChunk;
    fn next(&mut self) -> Option<UnpackedChunk> {
        let slice = self.slicer.take_slice()?;
        Some(block_on(UnpackedChunk::new(
            slice,
            self.settings,
            self.key.clone(),
        )))
    }
}

/// Trait for the setttings object associated with the Slicer
pub trait SlicerSettings<R: Read> {
    type Slicer: Slicer<R>;
    /// Given a reader, transforms this into its relevant slicer
    fn to_slicer(&self, reader: R) -> Self::Slicer;
}
