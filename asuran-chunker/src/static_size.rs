use super::{Chunker, ChunkerError};

use std::io::{BufReader, Bytes, Read};

/// Settings for a static chunk length `Chunker`
///
/// This is a pretty simple chunker, it simply splits the contents into `len` sized
/// chunks. Has a comparatively poor reduplication ratio, due to the boundary shift
/// problem, but it has more performance than just about anything else out there.
#[derive(Clone, Copy)]
pub struct StaticSize {
    pub len: usize,
}

impl Chunker for StaticSize {
    type Chunks = StaticSizeChunker;
    fn chunk_boxed(&self, read: Box<dyn Read + Send + 'static>) -> Self::Chunks {
        StaticSizeChunker {
            settings: *self,
            internal: BufReader::new(read).bytes(),
            next: None,
        }
    }
}

impl Default for StaticSize {
    /// Provides default settings with a chunk size of 64kiB
    fn default() -> Self {
        StaticSize { len: 65_536 }
    }
}

pub struct StaticSizeChunker {
    /// Settings for this `Chunker`
    settings: StaticSize,
    /// `Read` this `Chunker` is slicing over
    internal: Bytes<BufReader<Box<dyn Read + Send + 'static>>>,
    next: Option<std::io::Result<u8>>,
}

impl Iterator for StaticSizeChunker {
    type Item = Result<Vec<u8>, ChunkerError>;
    fn next(&mut self) -> Option<Self::Item> {
        let mut buffer = Vec::new();
        let mut next = if self.next.is_some() {
            self.next.take()
        } else {
            self.internal.next()
        };
        while next.is_some() && buffer.len() < self.settings.len {
            // This unwrap is safe because we just verified that it is a Some(T)
            let byte = next.unwrap();
            let byte = match byte {
                Ok(byte) => byte,
                Err(err) => return Some(Err(err.into())),
            };
            buffer.push(byte);
            next = self.internal.next();
        }
        if buffer.is_empty() {
            None
        } else {
            self.next = next;
            Some(Ok(buffer))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::prelude::*;
    use std::io::Cursor;

    // Provides a test slice 10 times the default max size in length
    fn get_test_data() -> Vec<u8> {
        let size = StaticSize::default().len * 10 + 10_000;
        let mut vec = vec![0_u8; size as usize];
        rand::thread_rng().fill_bytes(&mut vec);
        vec
    }

    // Data should be split into one or more chunks
    #[test]
    fn one_or_more_chunks() {
        let data = get_test_data();
        let cursor = Cursor::new(data);
        let chunker = StaticSize::default();
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
        let chunks = StaticSize::default()
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
        let chunks1 = StaticSize::default()
            .chunk(cursor1)
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        let cursor2 = Cursor::new(data);
        let chunks2 = StaticSize::default()
            .chunk(cursor2)
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();
        assert_eq!(chunks1, chunks2);
    }

    // Verifies that this `Chunker` does not produce chunks larger than its max size
    #[test]
    fn max_size() {
        let data = get_test_data();
        let max_size = StaticSize::default().len;

        let chunks = StaticSize::default()
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
        let min_size = StaticSize::default().len;

        let chunks = StaticSize::default()
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
