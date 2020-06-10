use crate::repository::backend::{BackendError, Result};
use crate::repository::{Chunk, ChunkSettings, Key};

use asuran_core::repository::chunk::{ChunkBody, ChunkHeader};

use serde::{Deserialize, Serialize};
use serde_cbor as cbor;
use uuid::Uuid;

use std::convert::TryInto;
use std::io::{Read, Seek, SeekFrom, Write};

/// Magic number used for asuran segment files
///
/// More or less completly arbitrary, but used to validate files
const MAGIC_NUMBER: [u8; 8] = *b"ASURAN_S";

/// Representation of the header at the start of each file
///
/// Designed to be bincoded directly into a spec compliant format with big endian
/// set
#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Header {
    magic_number: [u8; 8],
    implementation_uuid: [u8; 16],
    major: u16,
    minor: u16,
    patch: u16,
}

impl Header {
    /// Creates a new segment header with correct values for this version of libasuran
    pub fn new() -> Header {
        Self::default()
    }

    /// Checks if a header is valid for this version of libasuran
    ///
    /// Currently only checks the header
    pub fn validate(&self) -> bool {
        self.magic_number == MAGIC_NUMBER
    }

    /// Returns the implementation UUID
    pub fn uuid(&self) -> Uuid {
        Uuid::from_bytes(self.implementation_uuid)
    }

    /// Reconstructs the version string
    pub fn version_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Default for Header {
    /// Constructs a header using the correct values for this version of libasuran
    fn default() -> Header {
        Header {
            magic_number: MAGIC_NUMBER,
            implementation_uuid: *crate::IMPLEMENTATION_UUID.as_bytes(),
            major: crate::VERSION_PIECES[0],
            minor: crate::VERSION_PIECES[1],
            patch: crate::VERSION_PIECES[2],
        }
    }
}

/// Represents an entry in the Header Part of a segment
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SegmentHeaderEntry {
    pub header: ChunkHeader,
    pub start_offset: u64,
    pub end_offset: u64,
}

/// A view over the header portion of a segment
///
/// Will cache the state of the header internally and flush the changes either when
/// the `flush` method is called, or on drop.
///
/// It is strongly recommended to call the `flush` method before dropping this type.
/// The `Drop` impl will attempt to call `flush`, but will ignore any errors that
/// occur.
pub struct SegmentHeaderPart<T: Read + Write + Seek> {
    handle: T,
    entries: Vec<SegmentHeaderEntry>,
    settings: ChunkSettings,
    key: Key,
    changed: bool,
}

impl<T: Read + Write + Seek> SegmentHeaderPart<T> {
    /// Attempts to open the header part of a `Segment`.
    ///
    /// # Errors:
    ///
    /// Will error if decryption fails, the header file has a malformed chunk, or if
    /// some other IO error occurs.
    pub fn open(mut handle: T, key: Key, settings: ChunkSettings) -> Result<Self> {
        let len = handle.seek(SeekFrom::End(0))?;
        // if we are empty, we don't need to actually read anything
        if len > 0 {
            handle.seek(SeekFrom::Start(0))?;
            let chunk: Chunk = cbor::de::from_reader(&mut handle)?;
            let data = chunk.unpack(&key)?;
            let entries: Vec<SegmentHeaderEntry> = cbor::de::from_slice(&data[..])?;
            Ok(SegmentHeaderPart {
                handle,
                entries,
                settings,
                key,
                changed: false,
            })
        } else {
            Ok(SegmentHeaderPart {
                handle,
                entries: Vec::new(),
                settings,
                key,
                changed: true,
            })
        }
    }

    /// Flushes the in-memory buffer to disk.
    ///
    /// Will not do anything if no changes have been added.
    ///
    /// Will additionally reset the changed flag.
    ///
    /// # Errors:
    ///
    /// Will error if an I/O error occurs during writing.
    ///
    pub fn flush(&mut self) -> Result<()> {
        if self.changed {
            self.handle.seek(SeekFrom::Start(0))?;
            let data = cbor::ser::to_vec(&self.entries)?;
            let chunk = Chunk::pack(
                data,
                self.settings.compression,
                self.settings.encryption,
                self.settings.hmac,
                &self.key,
            );
            cbor::ser::to_writer(&mut self.handle, &chunk)?;
            self.changed = false;
            Ok(())
        } else {
            Ok(())
        }
    }

    /// Will return the chunk header information at the given index, if one exists
    pub fn get_header(&self, index: usize) -> Option<SegmentHeaderEntry> {
        self.entries.get(index).cloned()
    }

    /// Will insert the chunk header information and provide its index
    pub fn insert_header(&mut self, header: SegmentHeaderEntry) -> usize {
        let index = self.entries.len();
        self.entries.push(header);
        self.changed = true;
        index
    }
}

impl<T: Read + Write + Seek> Drop for SegmentHeaderPart<T> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// A view over the data portion of a segment.
pub struct SegmentDataPart<T> {
    handle: T,
    size_limit: u64,
}

impl<T: Read + Write + Seek> SegmentDataPart<T> {
    /// Will attempt to open the given handle as a `SegmentDataPart`
    ///
    /// # Errors
    ///
    /// - Will propagate any IO errors
    /// - Will return `Err(BackendError::SegmentError)` if the segment has a header and
    ///   it fails validation
    pub fn new(handle: T, size_limit: u64) -> Result<Self> {
        let mut s = SegmentDataPart { handle, size_limit };
        // Attempt to write the header
        let written = s.write_header()?;
        if written {
            // Segment was empty, pass along as is
            Ok(s)
        } else {
            // Attempt to read the header
            let header = s.read_header()?;
            // Validate it
            if header.validate() {
                Ok(s)
            } else {
                Err(BackendError::SegmentError(
                    "Segment failed header validation".to_string(),
                ))
            }
        }
    }

    /// If the segment has non-zero length, will do nothing and return Ok(false).
    ///
    /// Otherwise, will write the header to the segment file.
    ///
    /// # Errors
    ///
    /// Will return `Err` if any underlying I/O errors occur
    pub fn write_header(&mut self) -> Result<bool> {
        // Am i empty?
        let end = self.handle.seek(SeekFrom::End(0))?;
        if end == 0 {
            // If we are empty, then the handle is at the start of the file
            let header = Header::default();
            let mut config = bincode::config();
            config
                .big_endian()
                .serialize_into(&mut self.handle, &header)
                .map_err(|_| BackendError::Unknown("Header Serialization failed".to_string()))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Will attempt to read the header from the segment
    ///
    /// # Errors
    ///
    /// - Will return `Err(BackendError::Unknown)` if deserializing the header fails
    /// - Will propagate any I/O errors that occur
    pub fn read_header(&mut self) -> Result<Header> {
        self.handle.seek(SeekFrom::Start(0))?;
        let mut config = bincode::config();
        let header: Header = config
            .big_endian()
            .deserialize_from(&mut self.handle)
            .map_err(|_| BackendError::Unknown("Header deserialization failed".to_string()))?;
        Ok(header)
    }

    /// Returns the current size of the segment file
    ///
    /// # Errors
    ///
    /// Will propagate any I/O errors that occur
    pub fn size(&mut self) -> Result<u64> {
        let len = self.handle.seek(SeekFrom::End(0))?;
        Ok(len)
    }

    /// Returns the number of free bytes remaining in this segment
    pub fn free_bytes(&mut self) -> Result<u64> {
        let len = self.handle.seek(SeekFrom::End(0))?;
        Ok(self.size_limit - len)
    }

    pub fn read_chunk(&mut self, header: SegmentHeaderEntry) -> Result<Chunk> {
        let length: usize = (header.end_offset - header.start_offset)
            .try_into()
            .expect("Chunk size too big to fit in memory");
        let mut buffer = vec![0_u8; length];
        self.handle.seek(SeekFrom::Start(header.start_offset))?;
        self.handle.read_exact(&mut buffer[..])?;
        let body = ChunkBody(buffer);
        Ok(Chunk::unsplit(header.header, body))
    }

    pub fn write_chunk(&mut self, chunk: Chunk) -> Result<SegmentHeaderEntry> {
        let start_offset: u64 = self.handle.seek(SeekFrom::End(1))?;
        let end_offset: u64 = start_offset + chunk.get_bytes().len() as u64;
        let (header, body) = chunk.split();
        self.handle.write_all(&body.0[..])?;
        Ok(SegmentHeaderEntry {
            header,
            start_offset,
            end_offset,
        })
    }
}

/// Generic segment implementation wrapping any Read + Write + Seek
pub struct Segment<T: Read + Write + Seek> {
    data_handle: SegmentDataPart<T>,
    header_handle: SegmentHeaderPart<T>,
}

impl<T: Read + Write + Seek> Segment<T> {
    /// Creates a new segment given a reader and a maximum size
    pub fn new(
        data_handle: T,
        header_handle: T,
        size_limit: u64,
        chunk_settings: ChunkSettings,
        key: Key,
    ) -> Result<Segment<T>> {
        let data_handle = SegmentDataPart::new(data_handle, size_limit)?;
        let header_handle = SegmentHeaderPart::open(header_handle, key, chunk_settings)?;
        Ok(Segment {
            data_handle,
            header_handle,
        })
    }

    /// Returns the size in bytes of the segment
    pub fn size(&mut self) -> u64 {
        self.data_handle
            .size()
            .expect("Unable to read size from data handle. Please check file permissions.")
    }

    /// Returns the number of bytes of free space remaining in the segment
    pub fn free_bytes(&mut self) -> u64 {
        self.data_handle
            .free_bytes()
            .expect("Unable to read size from data handle. Please check file permissions.")
    }

    /// Reads the chunk with the specified index from the segment
    pub fn read_chunk(&mut self, index: u64) -> Result<Chunk> {
        let index: usize = index
            .try_into()
            .expect("Index provided to read_chunk larger than could possibly fit into memory");
        let entry = self.header_handle.get_header(index).ok_or_else(|| {
            BackendError::SegmentError(format!("Invalid index {} provided to read_chunk", index))
        })?;
        self.data_handle.read_chunk(entry)
    }

    pub fn write_chunk(&mut self, chunk: Chunk) -> Result<u64> {
        let entry = self.data_handle.write_chunk(chunk)?;
        let index = self.header_handle.insert_header(entry);
        Ok(index as u64)
    }

    pub fn read_header(&mut self) -> Result<Header> {
        self.data_handle.read_header()
    }

    pub fn flush(&mut self) -> Result<()> {
        self.header_handle.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn header_sanity() {
        let input = Header::new();

        let mut config = bincode::config();
        config.big_endian();
        let bytes = config.serialize(&input).unwrap();

        let output: Header = config.deserialize(&bytes).unwrap();

        println!("{:02X?}", output);
        println!("{:02X?}", bytes);
        println!("{}", output.version_string());

        assert!(output.validate());
        assert_eq!(input, output);
        assert_eq!(output.uuid(), crate::IMPLEMENTATION_UUID.clone());
        assert_eq!(
            output.version_string(),
            crate::VERSION.split("-").next().unwrap()
        );
    }

    #[test]
    fn segment_header_sanity() {
        let key = Key::random(32);
        let cursor = Cursor::new(Vec::<u8>::new());
        let header_cursor = Cursor::new(Vec::<u8>::new());
        let mut segment = Segment::new(
            cursor,
            header_cursor,
            100,
            ChunkSettings::lightweight(),
            key,
        )
        .unwrap();

        assert!(segment.read_header().unwrap().validate())
    }
}
