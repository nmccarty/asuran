//! A slicer cuts data into chunks based on some predefined method
//!
//! Most typical is content defined slicing, but format specific methods are also quite useful

use crate::repository::UnpackedChunk;
use std::boxed::Box;
use std::io::Read;

/// Describes something that can slice objects in to chunks in a defined, repeatable manner
///
/// Must store state (including the reader) internally
pub trait Slicer {
    /// Inserts a reader into the Slicer
    ///
    /// Should clear state and drop previous reader
    fn add_reader(&mut self, reader: Box<dyn Read>);
    /// Returns the next slice
    fn take_slice(&mut self) -> UnpackedChunk;
}
