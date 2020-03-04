use cfg_if::cfg_if;
#[cfg(feature = "lz4")]
use lz4::{Decoder, EncoderBuilder};
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::io::copy;
#[allow(unused_imports)]
use std::io::Cursor;
use thiserror::Error;
#[cfg(feature = "xz2")]
use xz2::read::{XzDecoder, XzEncoder};

/// Error describing things that can go wrong with compression/decompression
#[derive(Error, Debug)]
pub enum CompressionError {
    #[error("I/O Error")]
    IOError(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, CompressionError>;

/// Marker for the type of compression used by a particular chunk
#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Compression {
    NoCompression,
    ZStd { level: i32 },
    LZ4 { level: u32 },
    LZMA { level: u32 },
}

impl Compression {
    /// Compresses the data with the algorithm indicated and level by the variant of
    /// `self`
    ///
    /// # Panics
    ///
    /// Will panic if the user selects a compression algorithm for which support has not
    /// been compiled in, or if compression otherwise fails.
    #[allow(unused_variables)]
    pub fn compress(self, data: Vec<u8>) -> Vec<u8> {
        match self {
            Compression::NoCompression => data,
            Compression::ZStd { level } => {
                cfg_if! {
                    if #[cfg(feature = "zstd")] {
                        let mut output = Vec::<u8>::with_capacity(data.len());
                        zstd::stream::copy_encode(data.as_slice(), &mut output, level).unwrap();
                        output
                    } else {
                        unimplemented!("Asuran was not compiled with zstd support.")
                    }
                }
            }
            Compression::LZ4 { level } => {
                cfg_if! {
                    if #[cfg(feature = "lz4")] {
                        let ouput = Vec::<u8>::with_capacity(data.len());
                        let cursor = Cursor::new(ouput);
                        let mut encoder = EncoderBuilder::new().level(level).build(cursor).unwrap();
                        let mut data = Cursor::new(data);
                        copy(&mut data, &mut encoder).unwrap();
                        let (cursor, result) = encoder.finish();
                        result.unwrap();
                        cursor.into_inner()
                    } else {
                        unimplemented!("Asuran was not compiled with lz4 support.")
                    }
                }
            }
            Compression::LZMA { level } => {
                cfg_if! {
                    if #[cfg(feature = "xz2")] {
                        let input = Cursor::new(data);
                        let mut output = Cursor::new(Vec::new());
                        let mut compressor = XzEncoder::new(input, level);
                        copy(&mut compressor, &mut output).unwrap();
                        output.into_inner()
                    } else {
                        unimplemented!("Asuran was not compiled with lzma support")
                    }
                }
            }
        }
    }

    /// Decompresses the given data with the algorithm specified by the variant of
    /// `self`
    ///
    /// # Errors
    ///
    /// Will return `Err` if decompression fails.
    ///
    /// # Panics
    ///
    /// Will panic if the user selects a compression algorithm for which support has not
    /// been compiled in.
    #[allow(unused_variables)]
    pub fn decompress(self, data: Vec<u8>) -> Result<Vec<u8>> {
        match self {
            Compression::NoCompression => Ok(data),
            Compression::ZStd { .. } => {
                cfg_if! {
                    if #[cfg(feature = "zstd")] {
                        let mut output = Vec::<u8>::new();
                        zstd::stream::copy_decode(data.as_slice(), &mut output)?;
                        Ok(output)
                    } else {
                        unimplemented!("Asuran was not compiled with zstd support")
                    }
                }
            }
            Compression::LZ4 { .. } => {
                cfg_if! {
                    if #[cfg(feature = "lz4")] {
                        let mut output = Cursor::new(Vec::<u8>::new());
                        let mut decoder = Decoder::new(Cursor::new(data))?;
                        copy(&mut decoder, &mut output)?;
                        let (_output, result) = decoder.finish();
                        result?;
                        Ok(output.into_inner())
                    } else {
                        unimplemented!("Asuran was not compiled with lz4 support")
                    }
                }
            }
            Compression::LZMA { .. } => {
                cfg_if! {
                    if #[cfg(feature = "xz2")] {
                        let input = Cursor::new(data);
                        let mut output = Cursor::new(Vec::new());
                        let mut decompressor = XzDecoder::new(input);
                        copy(&mut decompressor, &mut output)?;
                        Ok(output.into_inner())
                    } else {
                        unimplemented!("Asuran was not compiled with lzma support")
                    }
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
        let compressed_bytes = compression.compress(data_bytes.to_vec());
        let decompressed_bytes = compression.decompress(compressed_bytes.clone()).unwrap();
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
