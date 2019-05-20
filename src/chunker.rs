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
use rand::prelude::*;
use std::collections::VecDeque;
use std::io::Read;

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
pub struct IteratedReader<R: Read> {
    /// Internal Reader
    reader: R,
    /// Hasher
    hasher: BuzHash,
    /// Hash Mask
    mask: u64,
    /// Minimum chunk size
    min_size: usize,
    /// Maximum chunk size
    max_size: usize,
    /// read buffer
    buffer: [u8; 8192],
    /// location of the cursor in the buffer
    cursor: usize,
    /// length of the data in the buffer
    buffer_len: usize,
    /// location of the cursor in the file
    location: usize,
    /// Flag for if end of file reached
    finished: bool,
}

impl<R: Read> Iterator for IteratedReader<R> {
    type Item = Slice;

    #[cfg_attr(feature = "profile", flame)]
    fn next(&mut self) -> Option<Slice> {
        if self.finished {
            None
        } else {
            let start = self.location;
            let mut end = self.location;
            let mut output = Vec::<u8>::new();
            let hasher = &mut self.hasher;
            hasher.reset();
            let mut split = false;
            while !split {
                if self.cursor < self.buffer_len {
                    let byte = self.buffer[self.cursor];
                    output.push(byte);
                    let hash = hasher.hash_byte(byte);
                    let len = output.len();
                    if (hash & self.mask) == 0 && (len >= self.min_size) && (len <= self.max_size) {
                        split = true;
                        end = self.location;
                    }

                    self.location += 1;
                    self.cursor += 1;
                } else {
                    self.cursor = 0;
                    let result = self.reader.read(&mut self.buffer);
                    match result {
                        Err(_) => {
                            split = true;
                            end = self.location;
                            self.finished = true;
                        }
                        Ok(0) => {
                            split = true;
                            end = self.location;
                            self.finished = true;
                        }
                        Ok(n) => {
                            self.buffer_len = n;
                        }
                    }
                }
            }

            Some(Slice {
                data: output,
                start: start as u64,
                end: end as u64,
            })
        }
    }
}

/// Stores chunker settings for easy reuse
#[derive(Clone)]
pub struct Chunker {
    /// Hash Mask
    mask: u64,
    /// Hasher
    hasher: BuzHash,
    /// Mask bits count
    mask_bits: u32,
}

impl Chunker {
    /// Creates a new chunker with the given window and mask bits
    pub fn new(window: u64, mask_bits: u32, nonce: u64) -> Chunker {
        Chunker {
            mask: 2_u64.pow(mask_bits) - 1,
            hasher: BuzHash::new(nonce, window as u32),
            mask_bits,
        }
    }

    /// Produces an iterator over the slices in a file
    ///
    /// Will make a copy of the internal hashser
    pub fn chunked_iterator<R: Read>(&self, reader: R) -> IteratedReader<R> {
        IteratedReader {
            reader,
            hasher: self.hasher.clone(),
            mask: self.mask,
            min_size: 2_usize.pow(self.mask_bits - 2),
            max_size: 2_usize.pow(self.mask_bits + 2),
            buffer: [0_u8; 8192],
            cursor: 0,
            buffer_len: 0,
            location: 0,
            finished: false,
        }
    }
}

#[derive(Clone)]
pub struct BuzHash {
    hash: u64,
    table: [u64; 256],
    window_size: u32,
    buffer: VecDeque<u8>,
    count: u32,
}

impl BuzHash {
    pub fn new(nonce: u64, window_size: u32) -> BuzHash {
        let mut table = [0_u64; 256];
        let mut rand = SmallRng::seed_from_u64(nonce);
        for i in 0..256 {
            let value: u64 = rand.gen();
            table[i] = value;
        }
        BuzHash {
            hash: 0,
            table,
            window_size,
            buffer: VecDeque::with_capacity(window_size as usize),
            count: 0,
        }
    }

    pub fn hash_byte(&mut self, byte: u8) -> u64 {
        // Determine if removal is needed
        if self.count >= self.window_size {
            let hash = self.hash.rotate_left(1);
            let head = self.buffer.pop_front().unwrap();
            let head = self.table[head as usize].rotate_left(self.window_size);
            let tail = self.table[byte as usize];
            self.hash = hash ^ head ^ tail;
        } else {
            self.count += 1;
            let hash = self.hash.rotate_left(1);
            let tail = self.table[byte as usize];
            self.hash = hash ^ tail;
        }

        self.buffer.push_back(byte);
        self.hash
    }

    pub fn reset(&mut self) {
        self.hash = 0;
        self.count = 0;
        self.buffer.clear();
    }
}
