use super::{Chunker, ChunkerError};
use fastcdc;
use std::io::Read;

/// Settings for a fastcdc `Chunker`
///
/// These are limited to `usize`, and not `u64`, because this implementation makes extensive use of in
/// memory buffers of size `max_size`
#[derive(Clone, Copy)]
pub struct FastCDC {
    pub min_size: usize,
    pub max_size: usize,
    pub avg_size: usize,
}

impl Chunker for FastCDC {
    type Chunks = FastCDCChunker;
    fn chunk_boxed(&self, read: Box<dyn Read + 'static>) -> Self::Chunks {
        FastCDCChunker {
            settings: *self,
            buffer: vec![0_u8; self.max_size],
            length: 0,
            read,
            eof: false,
        }
    }
}

impl Default for FastCDC {
    fn default() -> Self {
        FastCDC {
            min_size: 32_768,
            avg_size: 65_536,
            max_size: 131_072,
        }
    }
}

pub struct FastCDCChunker {
    /// The settings used for this `Chunker`
    settings: FastCDC,
    /// The in memory buffer used to hack the chosen FastCDC implementation into working
    ///
    /// This must always be kept at a size of `max_size`
    buffer: Vec<u8>,
    /// The length of the data in the buffer
    length: usize,
    /// The reader this `Chunker` is slicing
    read: Box<dyn Read + 'static>,
    /// Has the reader hit EoF?
    eof: bool,
}

impl FastCDCChunker {
    /// Drains a specified number of bytes from the reader, and refills it back up to `max_size` with
    /// zeros.
    ///
    /// Additonally updates the pointer to the new end of the vec
    ///
    /// # Errors
    ///
    /// Returns `ChunkerError::InternalError` if the given count of bytes to drain is greater than the
    /// current size of the used region of the buffer.
    ///
    /// # Panics
    ///
    /// Panics if the internal buffer's length is not max_size. This is an invariant, and the end
    /// consumer of the struct should never be exposed to this error.
    fn drain_bytes(&mut self, count: usize) -> Result<Vec<u8>, ChunkerError> {
        assert!(self.buffer.len() == self.settings.max_size);
        if count > self.length {
            Err(ChunkerError::InternalError(format!(
                "Invalid count given to FastCDCChunker::drain_bytes. Count: {}, Length: {}",
                count, self.length
            )))
        } else {
            // Drain the bytes from the vec
            let output = self.buffer.drain(..count).collect::<Vec<_>>();
            // Update the length
            self.length = self.length - count;
            // Resize the buffer back to max_size
            self.buffer.resize(self.settings.max_size, 0_u8);
            // Return the drained bytes
            Ok(output)
        }
    }

    /// Returns true if the internal buffer is empty
    fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Attempts fill the buffer back up with bytes from the read
    ///
    /// Returns the number of bytes read. Will not attempt to read bytes if EoF has already been
    /// encountered.
    ///
    /// # Errors
    ///
    /// Returns `ChunkerError::IOError` if the reader provides any error value during reading
    ///
    /// # Panics
    ///
    /// Panics if the internal buffer's length is not max_size. This is an invariant, and the end
    /// consumer of the struct shuold never be exposed to this error.
    fn read_bytes(&mut self) -> Result<usize, ChunkerError> {
        assert!(self.buffer.len() == self.settings.max_size);
        if self.eof {
            Ok(0)
        } else {
            let mut total_bytes = 0;
            // While we have not hit eof, and there is still room in the buffer, keep reading
            while !self.eof && self.length < self.settings.max_size {
                // read some bytes
                let bytes_read = self.read.read(&mut self.buffer[self.length..])?;
                // Update the length
                self.length += bytes_read;
                // If the number of bytes read was zero, set the eof flag
                if bytes_read == 0 {
                    self.eof = true;
                }
                // Update the total
                total_bytes += bytes_read;
            }
            Ok(total_bytes)
        }
    }

    /// Uses the fastcdc algorithim to produce the next chunk of data.
    ///
    /// # Errors
    ///
    /// Returns `ChunkerError::Empty` if EoF has been hit
    ///
    /// # Panics
    ///
    /// Panics if the internal buffer's length is not max_size. This is an invariant, and the end
    /// consumer of the struct should never be exposed to this error.
    fn next_chunk(&mut self) -> Result<Vec<u8>, ChunkerError> {
        assert_eq!(self.buffer.len(), self.settings.max_size);
        // First, perform a read, to make sure the buffer is as full as it can be
        self.read_bytes()?;
        // Check to see if we are empty, if so, return early
        if self.is_empty() {
            Err(ChunkerError::Empty)
        } else {
            // Attempt to produce our slice
            let mut slicer = fastcdc::FastCDC::new(
                &self.buffer[..self.length],
                self.settings.min_size,
                self.settings.avg_size,
                self.settings.max_size,
            );
            if let Some(chunk) = slicer.next() {
                let result = self.drain_bytes(chunk.length)?;
                Ok(result)
            } else {
                // We really shouldnt be here, since we ruled out the empty case, eairlier but we
                // will error anyway
                Err(ChunkerError::Empty)
            }
        }
    }
}

impl Iterator for FastCDCChunker {
    type Item = Result<Vec<u8>, ChunkerError>;

    fn next(&mut self) -> Option<Result<Vec<u8>, ChunkerError>> {
        let slice = self.next_chunk();
        if let Err(ChunkerError::Empty) = slice {
            None
        } else {
            Some(slice)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::prelude::*;
    use std::io::Cursor;

    // Provides a test slice 512KiB in length
    fn get_test_data() -> Vec<u8> {
        let size = 524_288;
        let mut vec = vec![0_u8; size];
        rand::thread_rng().fill_bytes(&mut vec);
        vec
    }

    // Data should be split into one or more chunks.
    //
    // In this case, the data is larger than max size, so it should be more than one chunk
    #[test]
    fn one_or_more_chunks() {
        let data = get_test_data();
        let cursor = Cursor::new(data);
        let chunker = FastCDC::default();
        let chunks = chunker
            .chunk(cursor)
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        assert!(chunks.len() > 1);
    }

    // Data should be identical after reassembaly by simple concatenation
    #[test]
    fn reassemble_data() {
        let data = get_test_data();
        let cursor = Cursor::new(data.clone());
        let chunks = FastCDC::default()
            .chunk(cursor)
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        let rebuilt: Vec<u8> = chunks.concat();
        assert_eq!(data, rebuilt);
    }

    // Running the chunker over the same data twice should result in identical chunks
    #[test]
    fn identical_chunks() {
        let data = get_test_data();
        let cursor1 = Cursor::new(data.clone());
        let chunks1 = FastCDC::default()
            .chunk(cursor1)
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        let cursor2 = Cursor::new(data);
        let chunks2 = FastCDC::default()
            .chunk(cursor2)
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(chunks1, chunks2);
    }
}
