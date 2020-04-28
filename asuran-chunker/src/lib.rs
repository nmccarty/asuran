//! API for describing types that can slice data into component slices in a repeatable manner
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::similar_names)]
#![allow(clippy::pub_enum_variant_names)]
#![allow(clippy::missing_errors_doc)]

pub mod buzhash;
pub mod fastcdc;
pub mod static_size;

pub use self::buzhash::*;
pub use self::fastcdc::*;
pub use self::static_size::*;

use thiserror::Error;

use std::io;

#[cfg(feature = "streams")]
use futures::channel::mpsc;
#[cfg(feature = "streams")]
use futures::sink::SinkExt;
#[cfg(feature = "streams")]
use smol::block_on;
#[cfg(feature = "streams")]
use std::thread;

#[derive(Error, Debug)]
pub enum ChunkerError {
    #[error("Provider IO error")]
    IOError(#[from] io::Error),
    #[error("Internal Chunker Error")]
    InternalError(String),
    #[error("Slicer incorrectly applied to empty data")]
    Empty,
}

use std::io::{Cursor, Read};

/// Describes something that can slice objects in a defined, repeatable manner
///
/// Chunkers must meet the following properties:
/// 1.) Data must be split into one or more chunks
/// 2.) Data must be identical to original after a simple reconstruction by concatenation
/// 3.) The same data and settings must produce the same slices every time
/// 4.) Chunkers (that have a max size) should not produce any chunks larger than their `max_size`
/// 5.) Chunkers (that have a min size) should produce, at most, 1 slice smaller than its `min_size`,
///  and should only do as such when there is not enough data left to produce a min size chunk
///
/// For the time being given the lack of existential types, Chunkers use Box<dyn Read + 'static>.
///
/// If/when existential types get stabilized in a way that helps, this will be switched to an
/// existential type, to drop the dynamic dispatch.
///
/// Chunkers should, ideally, contain only a small number of settings for the chunking algorithm,
/// and should there for be cloneable with minimal overhead. Ideally, they should implement copy,
/// but that is not supplied as a bound to increase the flexibility in implementation
///
/// The Send bound on the Read is likely temporary, it is currently required to make the streams
/// feature work properly.
pub trait Chunker: Clone {
    /// The return type of the functions in this trait is an iterator over the chunks of their
    /// input.
    ///
    /// The returned iterator must be owned, hence the 'static bound.
    type Chunks: Iterator<Item = Result<Vec<u8>, ChunkerError>> + 'static;
    /// Core function, takes a boxed owned Read and produces an iterator of Vec<u8> over it
    fn chunk_boxed(&self, read: Box<dyn Read + Send + 'static>) -> Self::Chunks;
    /// Convenience function that boxes a bare Read for you, and passes it to `chunk_boxed`
    ///
    /// This will be the primary source of interaction wth the API for most use cases
    fn chunk<R: Read + Send + 'static>(&self, read: R) -> Self::Chunks {
        let boxed: Box<dyn Read + Send + 'static> = Box::new(read);
        self.chunk_boxed(boxed)
    }
    /// Convenience function that boxes an AsRef<[u8]> wrapped in a cursor and passes it to
    /// `chunk_boxed`. Implementations are encouraged to overwrite when sensible.
    ///
    /// This method is provided to ensure API compatibility when implementations are using memory
    /// mapped io or the like. When chunkers can sensibly override this, they are encouraged to, as
    /// it would otherwise result in a performance overhead for consumers using memmaped IO.
    fn chunk_slice<R: AsRef<[u8]> + Send + 'static>(&self, slice: R) -> Self::Chunks {
        let cursor = Cursor::new(slice);
        let boxed: Box<dyn Read + Send + 'static> = Box::new(cursor);
        self.chunk_boxed(boxed)
    }
}

/// Asynchronous version of `Chunker`
///
/// Only available if the streams feature is enabled.
///
/// Works by performing the chunking in an async task, falling through to the implementation in
/// `Chunker`, and passing the results over an mspc channel
#[cfg(feature = "streams")]
pub trait AsyncChunker: Chunker + Send + Sync {
    /// Async version of `Chunker::chunk_boxed`
    fn async_chunk_boxed(
        &self,
        read: Box<dyn Read + Send + 'static>,
        queue_depth: usize,
    ) -> mpsc::Receiver<Result<Vec<u8>, ChunkerError>>;
    /// Async version of `Chunker::chunk`
    fn async_chunk<R: Read + Send + 'static>(
        &self,
        read: R,
        queue_depth: usize,
    ) -> mpsc::Receiver<Result<Vec<u8>, ChunkerError>>;
    /// Async version of `Chunker::chunk_slice`
    fn async_chunk_slice<R: AsRef<[u8]> + Send + 'static>(
        &self,
        slice: R,
        queue_depth: usize,
    ) -> mpsc::Receiver<Result<Vec<u8>, ChunkerError>>;
}

#[cfg(feature = "streams")]
impl<T> AsyncChunker for T
where
    T: Chunker + Send + Sync,
    <T as Chunker>::Chunks: Send,
{
    fn async_chunk_boxed(
        &self,
        read: Box<dyn Read + Send + 'static>,
        queue_depth: usize,
    ) -> mpsc::Receiver<Result<Vec<u8>, ChunkerError>> {
        let (mut input, output) = mpsc::channel(queue_depth);
        let mut iter = self.chunk_boxed(read);
        thread::spawn(move || {
            while let Some(chunk) = iter.next() {
                // If we are here, and sending to the channel fails, we have no sensible way to
                // recover, as we have lost communication with the outside world
                block_on(input.send(chunk)).expect("Chunker to communicate with outside world.");
            }
        });
        output
    }
    fn async_chunk<R: Read + Send + 'static>(
        &self,
        read: R,
        queue_depth: usize,
    ) -> mpsc::Receiver<Result<Vec<u8>, ChunkerError>> {
        let (mut input, output) = mpsc::channel(queue_depth);
        let mut iter = self.chunk(read);
        thread::spawn(move || {
            while let Some(chunk) = iter.next() {
                // If we are here, and sending to the channel fails, we have no sensible way to
                // recover, as we have lost communication with the outside world
                block_on(input.send(chunk)).expect("Chunker to communicate with outside world.");
            }
        });
        output
    }
    fn async_chunk_slice<R: AsRef<[u8]> + Send + 'static>(
        &self,
        slice: R,
        queue_depth: usize,
    ) -> mpsc::Receiver<Result<Vec<u8>, ChunkerError>> {
        let (mut input, output) = mpsc::channel(queue_depth);
        let mut iter = self.chunk_slice(slice);
        thread::spawn(move || {
            while let Some(chunk) = iter.next() {
                // If we are here, and sending to the channel fails, we have no sensible way to
                // recover, as we have lost communication with the outside world
                block_on(input.send(chunk)).expect("Chunker to communicate with outside world.");
            }
        });
        output
    }
}
