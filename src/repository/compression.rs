use std::io;

/// Compression algorithim
#[derive(Copy, Clone)]
pub enum Compression {
    NoCompression,
    ZStd { level: i32 },
}

impl Compression {
    /// Will compress the given data
    ///
    /// Panics if compression fails
    pub fn compress(&self, data: &[u8]) -> Vec<u8> {
        match self {
            Compression::NoCompression => data.to_vec(),
            Compression::ZStd { level } => {
                let mut output = Vec::<u8>::new();
                zstd::stream::copy_encode(data, &mut output, *level).unwrap();
                output
            }
        }
    }

    /// Decompresses the given data
    ///
    /// Will return none if decompression fails
    pub fn decompress(&self, data: &[u8]) -> Option<Vec<u8>> {
        match self {
            Compression::NoCompression => Some(data.to_vec()),
            Compression::ZStd { .. } => {
                let mut output = Vec::<u8>::new();
                let result = zstd::stream::copy_decode(data, &mut output);
                if let Err(_) = result {
                    None
                } else {
                    Some(output)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str;

    #[test]
    fn test_zstd() {
        let compression = Compression::ZStd { level: 6 };

        let data_string =
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";
        let data_bytes = data_string.as_bytes();
        let compressed_bytes = compression.compress(data_bytes);
        let decompressed_bytes = compression.decompress(&compressed_bytes).unwrap();
        let decompressed_string = str::from_utf8(&decompressed_bytes).unwrap();

        println!("Input string: {}", data_string);
        println!("Input bytes: \n{:X?}", data_bytes);
        println!("Original length: {}", data_bytes.len());
        println!("Compressed bytes: \n{:X?}", compressed_bytes);
        println!("Compressed length: {}", compressed_bytes.len());
        println!("Decompressed bytes: \n{:X?}", decompressed_bytes);
        println!("Decompressed string: {}", decompressed_string);

        assert_eq!(data_string, decompressed_string);
    }

}
