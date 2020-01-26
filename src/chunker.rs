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
//! splits when the last `mask_bits` of the resulting hash are all zero.
//!
//! This results in a `2^mask_bits` byte statistical slice length, minimum and
//! maximum chunk sizes are set at 2^(mask_bits-2) bytes and `2^(mask_bits+2)`
//! bytes, respectivly, to prevent overly large or small slices, and to provide
//! some measure of predictibility.
use std::io::{Empty, Read};

pub mod slicer;
pub use self::slicer::{ChunkIterator, Slicer, SlicerSettings};
use futures::channel::mpsc;
use futures::sink::SinkExt;
use tokio::task;

#[cfg(feature = "profile")]
use flamer::*;

/// Stores the data in a slice/chunk, as well as providing information about
/// the location of the slice in the file.
pub struct Slice {
    pub data: Vec<u8>,
    pub start: u64,
    pub end: u64,
}

/// Apply content based slicing over a Read, and iterate throught the slices
///
/// Slicing is applied as the iteration proceeds, so each byte is only read once
pub struct IteratedReader<R, S> {
    /// Internal chunk iterator
    chunk_iterator: ChunkIterator<R, S>,
    /// Offset used for calcuating slice ends
    offset: u64,
}

impl<R: Read + Send, S: Slicer<R>> Iterator for IteratedReader<R, S> {
    type Item = Slice;

    #[cfg_attr(feature = "profile", flame)]
    fn next(&mut self) -> Option<Slice> {
        let data = self.chunk_iterator.next()?;
        let start = self.offset;
        self.offset = start + (data.len() as u64);
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
    pub fn chunked_iterator<R>(&self, reader: R, offset: u64) -> IteratedReader<R, impl Slicer<R>>
    where
        R: Read + Send,
        S: SlicerSettings<R>,
    {
        let slicer = <S as SlicerSettings<R>>::to_slicer(&self.settings, reader);
        let chunk_iterator = slicer.into_chunk_iter();
        IteratedReader {
            chunk_iterator,
            offset,
        }
    }

    /// Creates an asyncronous stream over the slices in an object
    ///
    /// This is an async version of chunked_iterator that works by calling .next in a blocking task
    ///
    /// The task will continously chunk the data in the background, and push it into a mpsc channel
    pub fn chunked_stream<R>(&self, reader: R, offset: u64) -> mpsc::Receiver<Slice>
    where
        R: Read + Send + 'static,
        S: SlicerSettings<R> + 'static,
    {
        let (mut input, output) = mpsc::channel(100);
        let mut iter = self.chunked_iterator(reader, offset);
        task::spawn(async move {
            let mut next = iter.next();
            while let Some(item) = next {
                input.send(item).await.unwrap();
                next = task::block_in_place(|| iter.next());
            }
        });
        output
    }
}
