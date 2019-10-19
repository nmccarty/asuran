use super::Slicer;
use crate::repository::chunk::*;
use fastcdc;
use std::boxed::Box;
use std::io::Read;

pub struct FastCDC {
    reader: Option<Box<dyn Read>>,
    min_size: usize,
    max_size: usize,
    avg_size: usize,
    buffer: Vec<u8>,
}

impl FastCDC {
    pub fn new(min_size: usize, max_size: usize, avg_size: usize) -> FastCDC {
        FastCDC {
            reader: None,
            min_size,
            max_size,
            avg_size,
            buffer: Vec::new(),
        }
    }

    pub fn new_defaults() -> FastCDC {
        Self::new(16384, 65536, 32768)
    }
}

impl Slicer for FastCDC {
    fn add_reader(&mut self, reader: Box<dyn Read>) {
        self.reader = Some(reader);
    }
    fn take_slice(&mut self) -> Option<Vec<u8>> {
        if let Some(reader) = &mut self.reader {
            // Fill buffer if it needs to be filled
            if self.buffer.len() < self.max_size {
                let mut tiny_buf = [0_u8; 1024];
                let mut eof = false;
                while !eof && self.buffer.len() < self.max_size {
                    let bytes_read = reader.read(&mut tiny_buf).expect("Unable to read data");
                    if bytes_read == 0 {
                        eof = true;
                    } else {
                        self.buffer.extend_from_slice(&tiny_buf[..bytes_read]);
                    }
                }
            }

            // find chunk edge
            let mut chunker =
                fastcdc::FastCDC::new(&self.buffer, self.min_size, self.avg_size, self.max_size);
            // Attempt to get a chunk
            if let Some(chunk) = chunker.next() {
                let length = chunk.length;
                let output = self.buffer.drain(..length).collect();
                Some(output)
            } else {
                // in this case the buffer is emtpy and we have no more data
                None
            }
        } else {
            None
        }
    }
}

impl Iterator for FastCDC {
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
            let mut slicer = FastCDC::new_defaults();
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
            let mut slicer = FastCDC::new_defaults();
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
            let mut slicer1 = FastCDC::new_defaults();
            slicer1.add_reader(Box::new(cursor1));
            let sliced1: Vec<Vec<u8>> = slicer1.collect();

            let cursor2 = Cursor::new(data.clone());
            let mut slicer2 = FastCDC::new_defaults();
            slicer2.add_reader(Box::new(cursor2));
            let sliced2: Vec<Vec<u8>> = slicer2.collect();

            sliced1 == sliced2
        }
        let mut qc = QuickCheck::with_gen(StdThreadGen::new(1048576)).tests(20);
        qc.quickcheck(prop as fn(Vec<u8>) -> bool);
    }
}
