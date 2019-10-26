//! The chunker is responsible for dividing objects into chunks for
//! deduplication and storage
//!
//! The chunker uses a rolling hash (currently a derivitive of buszhash) to
//! perform content based slicing. Any single byte change in the object should,
//! ideally, only result in having to re-write the chunk it is in, and maybe the
//! one after it. It should not affect deduplication for the rest of the file.
//! In practice, this can sometimes get confounded by the minimum and maxiumum
//! chunk sizes.
//!
//! The file is sliced by rolling each slice through the hasher, and generating
//! splits when the last mask_bits of the resulting hash are all zero.
//!
//! This results in a 2^mask_bits byte statistical slice length, minimum and
//! maximum chunk sizes are set at 2^(mask_bits-2) bytes and 2^(mask_bits+2)
//! bytes, respectivly, to prevent overly large or small slices, and to provide
//! some measure of predictibility.
use crate::repository::{ChunkSettings, Key, UnpackedChunk};
use std::io::{Empty, Read};

pub mod slicer;
pub use self::slicer::{ChunkIterator, Slicer, SlicerSettings};

#[cfg(feature = "profile")]
use flamer::*;

/// Stores the data in a slice/chunk, as well as providing information about
/// the location of the slice in the file.
pub struct Slice {
    pub data: UnpackedChunk,
    pub start: u64,
    pub end: u64,
}

/// Apply content based slicing over a Read, and iterate throught the slices
///
/// Slicing is applied as the iteration proceeds, so each byte is only read once
pub struct IteratedReader<R: Read, S: Slicer<R>> {
    /// Internal chunk iterator
    chunk_iterator: ChunkIterator<R, S>,
    /// Offset used for calcuating slice ends
    offset: u64,
}

impl<R: Read, S: Slicer<R>> Iterator for IteratedReader<R, S> {
    type Item = Slice;

    #[cfg_attr(feature = "profile", flame)]
    fn next(&mut self) -> Option<Slice> {
        let data = self.chunk_iterator.next()?;
        let start = self.offset;
        self.offset = start + (data.data().len() as u64);
        let end = self.offset - 1;

        Some(Slice { data, start, end })
    }
}

/// Stores chunker settings for easy reuse
#[derive(Clone)]
pub struct Chunker<S> {
    /// Internal slicer settings
    settings: S,
}

impl<S: SlicerSettings<Empty>> Chunker<S> {
    /// Creates a new chunker with settings of the given slicer
    pub fn new(settings: S) -> Chunker<S> {
        Chunker { settings }
    }

    /// Produces an iterator over the slices in an object
    ///
    /// Requries a reader over the object and the offset, in bytes, from the start of the object,
    /// as well as the settings and key used for chunk ID generation
    pub fn chunked_iterator<R>(
        &self,

        reader: R,
        offset: u64,
        settings: &ChunkSettings,
        key: &Key,
    ) -> IteratedReader<R, impl Slicer<R>>
    where
        R: Read,
        S: SlicerSettings<R>,
    {
        let slicer = <S as SlicerSettings<R>>::to_slicer(&self.settings, reader);
        let chunk_iterator = slicer.into_chunk_iter(settings.clone(), key.clone());
        IteratedReader {
            chunk_iterator,
            offset,
        }
    }
}
