use super::Slicer;
use rand::prelude::*;
use std::boxed::Box;
use std::collections::VecDeque;
use std::io::Read;

pub struct BuzHash {
    /// Internal reader
    reader: Option<Box<dyn Read>>,
    /// BuzHash Table
    table: [u64; 256],
    /// Size of the buzhash window in bytes
    window_size: u32,
    /// Buffer used by buzhash
    hash_buffer: VecDeque<u8>,
    /// Buffer over the reader
    buffer: [u8; 8192],
    /// length of the data in the buffer
    buffer_len: usize,
    /// Bytes in the buzhash buffer
    count: u32,
    /// Mask used for determining a split
    mask: u64,
    /// Number of bits in the mask
    mask_bits: u32,
    /// Minimum size of a chunk
    min_size: usize,
    /// Maximum size of a chunk
    max_size: usize,
    /// Current hash value
    hash: u64,
    /// Location in the reader buffer
    cursor: usize,
    /// Marker for if we have hit EoF
    finished: bool,
}

impl BuzHash {
    pub fn new(nonce: u64, window_size: u32, mask_bits: u32) -> BuzHash {
        let mut table = [0_u64; 256];
        let mut rand = SmallRng::seed_from_u64(nonce);
        for i in table.iter_mut() {
            *i = rand.gen();
        }
        BuzHash {
            reader: None,
            table,
            window_size,
            hash_buffer: VecDeque::with_capacity(window_size as usize),
            buffer: [0_u8; 8192],
            buffer_len: 0,
            count: 0,
            mask_bits,
            min_size: 2_usize.pow(mask_bits - 2),
            max_size: 2_usize.pow(mask_bits + 2),
            mask: 2_u64.pow(mask_bits) - 1,
            hash: 0,
            cursor: 0,
            finished: false,
        }
    }

    pub fn new_defaults(nonce: u64) -> BuzHash {
        Self::new(nonce, 4095, 21)
    }

    pub fn hash_byte(&mut self, byte: u8) -> u64 {
        // determine if removal is needed
        if self.count >= self.window_size {
            let hash = self.hash.rotate_left(1);
            let head = self.hash_buffer.pop_front().unwrap();
            let head = self.table[head as usize].rotate_left(self.window_size);
            let tail = self.table[byte as usize];
            self.hash = hash ^ head ^ tail;
        } else {
            self.count += 1;
            let hash = self.hash.rotate_left(1);
            let tail = self.table[byte as usize];
            self.hash = hash ^ tail;
        }

        self.hash_buffer.push_back(byte);
        self.hash
    }

    pub fn reset(&mut self) {
        self.hash = 0;
        self.count = 0;
        self.hash_buffer.clear();
    }
}

impl Slicer for BuzHash {
    fn add_reader(&mut self, reader: Box<dyn Read>) {
        self.reader = Some(reader);
    }
    fn take_slice(&mut self) -> Option<Vec<u8>> {
        self.reset();

        // Return none if we dont have a reader
        if self.reader.is_some() {
            if !self.finished {
                let mut output = Vec::<u8>::new();
                let mut split = false;
                while !split {
                    if self.cursor < self.buffer_len {
                        let byte = self.buffer[self.cursor];
                        output.push(byte);
                        let hash = self.hash_byte(byte);
                        let len = output.len();
                        if (hash & self.mask) == 0
                            && (len >= self.min_size)
                            && (len <= self.max_size)
                        {
                            split = true;
                        }

                        self.cursor += 1;
                    } else {
                        self.cursor = 0;
                        let result = self.reader.as_mut().unwrap().read(&mut self.buffer);
                        match result {
                            Err(_) => {
                                split = true;
                                self.finished = true;
                            }
                            Ok(0) => {
                                split = true;
                                self.finished = true;
                            }
                            Ok(n) => {
                                self.buffer_len = n;
                            }
                        }
                    }
                }
                Some(output)
            } else {
                None
            }
        } else {
            None
        }
    }
    fn copy_settings(&self) -> Self {
        BuzHash {
            reader: None,
            table: self.table,
            window_size: self.window_size,
            hash_buffer: VecDeque::with_capacity(self.window_size as usize),
            buffer: [0_u8; 8192],
            buffer_len: 0,
            count: 0,
            mask_bits: self.mask_bits,
            min_size: self.min_size,
            max_size: self.max_size,
            mask: self.mask,
            hash: 0,
            cursor: 0,
            finished: false,
        }
    }
}

impl Iterator for BuzHash {
    type Item = Vec<u8>;
    fn next(&mut self) -> Option<Vec<u8>> {
        self.take_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::*;
    use std::io::Cursor;

    #[test]
    fn one_or_more_chunks() {
        // Data should be sliced into one or more chunks
        fn prop(data: Vec<u8>) -> bool {
            let cursor = Cursor::new(data);
            let mut slicer = BuzHash::new_defaults(0);
            slicer.add_reader(Box::new(cursor));
            let sliced: Vec<Vec<u8>> = slicer.collect();
            sliced.len() >= 1
        }
        let mut qc = QuickCheck::with_gen(StdThreadGen::new(1048576)).tests(20);
        qc.quickcheck(prop as fn(Vec<u8>) -> bool);
    }

    #[test]
    fn reassemble_data() {
        // Data should be the same after reassembly
        fn prop(data: Vec<u8>) -> bool {
            let cursor = Cursor::new(data.clone());
            let mut slicer = BuzHash::new_defaults(0);
            slicer.add_reader(Box::new(cursor));
            let sliced: Vec<Vec<u8>> = slicer.collect();
            let rebuilt: Vec<u8> = sliced.concat();
            rebuilt == data
        }
        let mut qc = QuickCheck::with_gen(StdThreadGen::new(1048576)).tests(20);
        qc.quickcheck(prop as fn(Vec<u8>) -> bool);
    }

    #[test]
    fn identical_chunks() {
        // The same data should produce the same chunks
        fn prop(data: Vec<u8>) -> bool {
            let cursor1 = Cursor::new(data.clone());
            let mut slicer1 = BuzHash::new_defaults(0);
            slicer1.add_reader(Box::new(cursor1));
            let sliced1: Vec<Vec<u8>> = slicer1.collect();

            let cursor2 = Cursor::new(data.clone());
            let mut slicer2 = BuzHash::new_defaults(0);
            slicer2.add_reader(Box::new(cursor2));
            let sliced2: Vec<Vec<u8>> = slicer2.collect();

            sliced1 == sliced2
        }
        let mut qc = QuickCheck::with_gen(StdThreadGen::new(1048576)).tests(20);
        qc.quickcheck(prop as fn(Vec<u8>) -> bool);
    }
}
