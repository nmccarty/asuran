use super::{Chunker, ChunkerError};
use rand::prelude::*;
use std::collections::VecDeque;
use std::io::Read;

/// Settings for a `BuzHash` `Chunker`
///
/// Uses a randomized lookup table derived from a nonce provided by the repository
/// key material, to help provide resistance against a chunk size based
/// fingerprinting attack, for users who are concerned about such a thing.
///
/// This is a very tenuous mitigation for such an attack, borrowed from
/// [borg](https://borgbackup.readthedocs.io/en/stable/internals/security.html#fingerprinting),
/// and is, fundamentally, a temporary work around. The "correct" solution is to
/// implement a better repository structure, that does not leak chunk sizes.
#[derive(Clone, Copy)]
pub struct BuzHash {
    table: [u64; 256],
    window_size: u32,
    mask: u64,
    min_size: usize,
    max_size: usize,
}

impl BuzHash {
    pub fn new(nonce: u64, window_size: u32, mask_bits: u32) -> BuzHash {
        let mut table = [0_u64; 256];
        let mut rand = SmallRng::seed_from_u64(nonce);
        for i in table.iter_mut() {
            *i = rand.gen();
        }
        BuzHash {
            table,
            window_size,
            min_size: 2_usize.pow(mask_bits - 2),
            max_size: 2_usize.pow(mask_bits + 2),
            mask: 2_u64.pow(mask_bits) - 1,
        }
    }
}

impl BuzHash {
    pub fn with_default(nonce: u64) -> BuzHash {
        Self::new(nonce, 4095, 21)
    }

    #[cfg(test)]
    fn with_default_testing(nonce: u64) -> BuzHash {
        Self::new(nonce, 4095, 14)
    }
}

impl Chunker for BuzHash {
    type Chunks = BuzHashChunker;
    fn chunk_boxed(&self, read: Box<dyn Read + Send + 'static>) -> Self::Chunks {
        BuzHashChunker {
            settings: *self,
            read,
            buffer: VecDeque::new(),
            hash_buffer: VecDeque::new(),
            count: 0,
            hash: 0,
            eof: false,
        }
    }
}

pub struct BuzHashChunker {
    /// Settings for this `Chunker`
    settings: BuzHash,
    /// The reader this `Chunker` is slicing
    read: Box<dyn Read + Send + 'static>,
    /// The in memory buffer used for reading and popping bytes
    buffer: VecDeque<u8>,
    /// The buffer used by the rolling hash
    hash_buffer: VecDeque<u8>,
    /// Bytes in the hash buffer
    count: u32,
    /// The current hash value
    hash: u64,
    eof: bool,
}

impl BuzHashChunker {
    /// Hashes one byte and returns the new hash value
    fn hash_byte(&mut self, byte: u8) -> u64 {
        // determine if removal is needed
        if self.count >= self.settings.window_size {
            let hash = self.hash.rotate_left(1);
            let head = self.hash_buffer.pop_front().unwrap();
            let head = self.settings.table[head as usize].rotate_left(self.settings.window_size);
            let tail = self.settings.table[byte as usize];
            self.hash = hash ^ head ^ tail;
        } else {
            self.count += 1;
            let hash = self.hash.rotate_left(1);
            let tail = self.settings.table[byte as usize];
            self.hash = hash ^ tail;
        }

        self.hash_buffer.push_back(byte);
        self.hash
    }

    /// Reads up to `max_size` bytes into the internal buffer
    fn top_off_buffer(&mut self) -> Result<(), ChunkerError> {
        // Check to see if we need topping off
        if self.buffer.len() >= self.settings.max_size {
            Ok(())
        } else {
            // Create a temporary buffer that allows for the number of bytes needed to fill the
            // buffer. The result of this should not underflow as the buffer should never exceed
            // max_size in size.
            let tmp_buffer_size = self.settings.max_size - self.buffer.len();
            let mut tmp_buffer: Vec<u8> = vec![0_u8; tmp_buffer_size];
            let mut bytes_read = 0;
            while !self.eof && bytes_read < tmp_buffer_size {
                let local_bytes_read = self.read.read(&mut tmp_buffer[bytes_read..])?;
                // Update the length
                bytes_read += local_bytes_read;
                // If the number of bytes read was zero, set the eof flag
                if local_bytes_read == 0 {
                    self.eof = true;
                }
            }
            // Push the elements we read from the local buffer to the actual buffer
            for byte in tmp_buffer.iter().take(bytes_read) {
                self.buffer.push_back(*byte);
            }
            Ok(())
        }
    }

    /// Attempts to get another slice from the reader
    fn next_chunk(&mut self) -> Result<Vec<u8>, ChunkerError> {
        // Attempt to top off the buffer, this will ensure that we have either hit EoF or that there
        // are at least max_size bytes in the buffer
        self.top_off_buffer()?;
        // Check to see if there are any bytes in the buffer first. Since we just attempted to top
        // off the buffer, if we are still empty, that is because there are no more bytes to read.
        if self.buffer.is_empty() {
            // Go ahead and flag an empty status
            Err(ChunkerError::Empty)
        } else {
            // Check to see if we have flagged EoF, and the buffer is smaller than min_size
            if self.eof && self.buffer.len() <= self.settings.min_size {
                // In this case, there are no more bytes to read, and the remaining number of bytes
                // in the buffer is less that the minimum size slice we are allowed to produce, so
                // we just gather up those bytes and return them
                Ok(self.buffer.drain(..).collect())
            } else {
                let mut output = Vec::<u8>::new();
                let mut split = false;
                while !split && output.len() < self.settings.max_size && !self.buffer.is_empty() {
                    // Get the next byte and add it to the output
                    // This unwrap is valid because we ensure the buffer isnt empty in the loop
                    // conditional
                    let byte = self.buffer.pop_front().unwrap();
                    output.push(byte);
                    // Hash it
                    let hash = self.hash_byte(byte);
                    split = (hash & self.settings.mask == 0)
                        && (output.len() >= self.settings.min_size);
                }
                Ok(output)
            }
        }
    }
}

impl Iterator for BuzHashChunker {
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
    use std::io::Cursor;

    // Provides a test slice 5 times the default max size in length
    fn get_test_data() -> Vec<u8> {
        let size = BuzHash::with_default_testing(0).max_size * 10;
        let mut vec = vec![0_u8; size];
        rand::thread_rng().fill_bytes(&mut vec);
        vec
    }

    // Data should be split into one or more chunks.
    //
    // In this case, the data is larger than `max_size`, so it should be more than one chunk
    #[test]
    fn one_or_more_chunks() {
        let data = get_test_data();
        let cursor = Cursor::new(data);
        let chunker = BuzHash::with_default_testing(0);
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
        let chunks = BuzHash::with_default_testing(0)
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
        let chunks1 = BuzHash::with_default_testing(0)
            .chunk(cursor1)
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        let cursor2 = Cursor::new(data);
        let chunks2 = BuzHash::with_default_testing(0)
            .chunk(cursor2)
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(chunks1, chunks2);
    }

    // Verifies that this `Chunker` does not produce chunks larger than its max size
    #[test]
    fn max_size() {
        let data = get_test_data();
        let max_size = BuzHash::with_default_testing(0).max_size;

        let chunks = BuzHash::with_default_testing(0)
            .chunk(Cursor::new(data))
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();

        for chunk in chunks {
            assert!(chunk.len() <= max_size);
        }
    }

    // Verifies that this `Chunker`, at most, produces 1 under-sized chunk
    #[test]
    fn min_size() {
        let data = get_test_data();
        let min_size = BuzHash::with_default_testing(0).min_size;

        let chunks = BuzHash::with_default_testing(0)
            .chunk(Cursor::new(data))
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();

        let mut undersized_count = 0;
        for chunk in chunks {
            if chunk.len() < min_size {
                undersized_count += 1;
            }
        }

        assert!(undersized_count <= 1);
    }
}
