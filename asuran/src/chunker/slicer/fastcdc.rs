use super::{Slicer, SlicerSettings};
use fastcdc;
use std::io::BufReader;
use std::io::Read;

pub struct FastCDC<R: Read> {
    reader: Option<BufReader<R>>,
    min_size: usize,
    max_size: usize,
    avg_size: usize,
    buffer: Vec<u8>,
}

impl<R> FastCDC<R>
where
    R: Read + Send,
{
    pub fn new(min_size: usize, max_size: usize, avg_size: usize) -> FastCDC<R> {
        FastCDC {
            reader: None,
            min_size,
            max_size,
            avg_size,
            buffer: Vec::new(),
        }
    }

    pub fn new_defaults() -> FastCDC<R> {
        Self::new(57344, 65536, 73728)
    }
}

impl<R> Slicer<R> for FastCDC<R>
where
    R: Read + Send,
{
    type Settings = FastCDCSettings;
    fn add_reader(&mut self, reader: R) {
        self.reader = Some(BufReader::with_capacity(1_000_000, reader));
    }
    fn take_slice(&mut self) -> Option<Vec<u8>> {
        if let Some(reader) = &mut self.reader {
            // Fill buffer if it needs to be filled
            if self.buffer.len() < self.max_size {
                let mut tiny_buf = [0_u8; 8192];
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
    fn copy_settings(&self) -> Self::Settings {
        FastCDCSettings {
            min_size: self.min_size,
            max_size: self.max_size,
            avg_size: self.avg_size,
        }
    }
}

#[derive(Clone)]
pub struct FastCDCSettings {
    min_size: usize,
    max_size: usize,
    avg_size: usize,
}

impl<R> SlicerSettings<R> for FastCDCSettings
where
    R: Read + Send,
{
    type Slicer = FastCDC<R>;
    fn to_slicer(&self, reader: R) -> Self::Slicer {
        FastCDC {
            reader: Some(BufReader::with_capacity(1_000_000, reader)),
            min_size: self.min_size,
            max_size: self.max_size,
            avg_size: self.avg_size,
            buffer: Vec::new(),
        }
    }
}

impl<R> Iterator for FastCDC<R>
where
    R: Read + Send,
{
    type Item = Vec<u8>;
    fn next(&mut self) -> Option<Vec<u8>> {
        self.take_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::*;
    use std::io::{empty, Cursor};

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

    #[test]
    fn conversion_test() {
        // This test is genreally not needed, and only to make sure the type conversion works
        let mut slicer1 = FastCDC::new_defaults();
        slicer1.add_reader(empty());
        let settings = slicer1.copy_settings();
        let buffer = Cursor::new(Vec::<u8>::new());
        let _slicer2 = settings.to_slicer(buffer);

        assert!(true);
    }
}
