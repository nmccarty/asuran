//! API for describing types that can slice data into component slices in a repeatable manner

pub mod fastcdc;
pub use self::fastcdc::*;

use std::io;
use thiserror::Error;

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

/// Describes something that can slice objects in a defined, repeateable manner
///
/// Chunkers must meet three properties:
/// 1.) Data must be split into one or more chunks
/// 2.) Data must be identical to original after a simple reconstruction by concatenation
/// 3.) The same data and settings must produce the same slices every time
///
/// For the time being given the lack of existential types, Chunkers use Box<dyn Read + 'static>.
///
/// If/when existental types get stabilized in a way that helps, this will be switched to an
/// existential type, to drop the dynamic dispatch.
///
/// Chunkers should, ideally, contain only a small number of settings for the chunking algrothim,
/// and should there for be cloneable with minimal overhead. Ideally, they should implement copy,
/// but that is not supplied as a bound to increase the flexibilty in implementaion
pub trait Chunker: Clone {
    type Chunks: Iterator<Item = Result<Vec<u8>, ChunkerError>>;
    /// Core function, takes a boxed owned Read and produces an iterator of Vec<u8> over it
    fn chunk_boxed(&self, read: Box<dyn Read + 'static>) -> Self::Chunks;
    /// Convienice function that boxes a bare Read for you, and passes it to chunk_boxed
    ///
    /// This will be the primary source of interaction wth the API for most use cases
    fn chunk<R: Read + 'static>(&self, read: R) -> Self::Chunks {
        let boxed: Box<dyn Read + 'static> = Box::new(read);
        self.chunk_boxed(boxed)
    }
    /// Convience function that boxes an AsRef<[u8]> wrapped in a cursor and passes it to
    /// chunk_boxed. Implementations are encouraged to overwrite when sensible.
    ///
    /// This method is provided to ensure API compatibility when implementations are using memory
    /// mapped io or the like. When chunkers can sensibly override this, they are encouraged to, as
    /// it would otherwise result in a perforance overhead for consumers using memmaped IO.
    fn chunk_slice<R: AsRef<[u8]> + 'static>(&self, slice: R) -> Self::Chunks {
        let cursor = Cursor::new(slice);
        let boxed: Box<dyn Read + 'static> = Box::new(cursor);
        self.chunk_boxed(boxed)
    }
}
