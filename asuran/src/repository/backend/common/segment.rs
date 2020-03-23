use crate::repository::backend::{BackendError, Result, TransactionType};
use crate::repository::{Chunk, ChunkID, ChunkSettings, Key};
use asuran_core::repository::chunk::ChunkHeader;
use rmp_serde as rmps;
use serde::{Deserialize, Serialize};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use uuid::Uuid;

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
            let chunk: Chunk = rmps::decode::from_read(&mut handle)?;
            let data = chunk.unpack(&key)?;
            let entries: Vec<SegmentHeaderEntry> = rmps::decode::from_slice(&data[..])?;
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
            let data = rmps::encode::to_vec(&self.entries)?;
            let chunk = Chunk::pack(
                data,
                self.settings.compression,
                self.settings.encryption,
                self.settings.hmac,
                &self.key,
            );
            rmps::encode::write(&mut self.handle, &chunk)?;
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

/// Struct used to store a transaction inside a segment.
///
/// TODO: Change this to an enum
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Transaction {
    tx_type: TransactionType,
    id: ChunkID,
    chunk: Option<Chunk>,
}

impl Transaction {
    pub fn transaction_type(&self) -> TransactionType {
        self.tx_type
    }

    pub fn encode_insert(input: Chunk, id: ChunkID) -> Transaction {
        Transaction {
            tx_type: TransactionType::Insert,
            id,
            chunk: Some(input),
        }
    }

    pub fn encode_delete(id: ChunkID) -> Transaction {
        Transaction {
            tx_type: TransactionType::Delete,
            id,
            chunk: None,
        }
    }

    pub fn data(&self) -> Option<&Chunk> {
        self.chunk.as_ref()
    }

    pub fn take_data(self) -> Option<Chunk> {
        self.chunk
    }

    pub fn id(&self) -> ChunkID {
        self.id
    }
}

/// Generic segment implemenation wrapping any Read + Write + Seek
#[derive(Debug)]
pub struct Segment<T> {
    handle: T,
    size_limit: u64,
}

impl<T: Read + Write + Seek> Segment<T> {
    /// Creates a new segment given a reader and a maximum size
    pub fn new(handle: T, size_limit: u64) -> Result<Segment<T>> {
        let mut s = Segment { handle, size_limit };
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

    ///
    /// If the segment has non-zero length, will do nothing and return Ok(false)
    ///
    /// An error in reading/writing will bubble up.
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

    /// Attempts to read the header from the Segment
    ///
    /// Will return error if the read fails
    pub fn read_header(&mut self) -> Result<Header> {
        self.handle.seek(SeekFrom::Start(0))?;
        let mut config = bincode::config();
        let header: Header = config
            .big_endian()
            .deserialize_from(&mut self.handle)
            .map_err(|_| BackendError::Unknown("Header deserialization failed".to_string()))?;
        Ok(header)
    }

    /// Returns the size in bytes of the segment
    pub fn size(&mut self) -> u64 {
        self.handle.seek(SeekFrom::End(0)).unwrap()
    }

    pub async fn free_bytes(&mut self) -> u64 {
        let end = self.handle.seek(SeekFrom::End(0)).unwrap();
        self.size_limit - end
    }

    pub fn read_chunk(&mut self, start: u64) -> Result<Chunk> {
        self.handle.seek(SeekFrom::Start(start))?;
        let tx: Transaction = rmps::decode::from_read(&mut self.handle)?;
        let data = tx.take_data().ok_or_else(|| {
            BackendError::SegmentError("Read transaction does not have a chunk in it.".to_string())
        })?;
        Ok(data)
    }

    pub fn write_chunk(&mut self, chunk: Chunk, id: ChunkID) -> Result<u64> {
        let tx = Transaction::encode_insert(chunk, id);
        let start = self.handle.seek(SeekFrom::End(0))?;
        rmps::encode::write(&mut self.handle, &tx)?;
        Ok(start)
    }

    pub fn into_read_segment(self) -> ReadSegment<T> {
        ReadSegment {
            handle: BufReader::with_capacity(1_000_000, self.handle),
            size_limit: self.size_limit,
        }
    }

    pub fn into_write_segment(self) -> WriteSegment<T> {
        WriteSegment {
            handle: BufWriter::with_capacity(1_000_000, self.handle),
            size_limit: self.size_limit,
        }
    }
}

#[derive(Debug)]
/// Analogue of `Segement` that uses a `BufReader`, but can only allow read operations
pub struct ReadSegment<T> {
    handle: BufReader<T>,
    size_limit: u64,
}

impl<T: Read + Seek> ReadSegment<T> {
    pub fn read_chunk(&mut self, start: u64, _length: u64) -> Result<Chunk> {
        self.handle.seek(SeekFrom::Start(start))?;
        let tx: Transaction = rmps::decode::from_read(&mut self.handle)?;
        let data = tx.take_data().ok_or_else(|| {
            BackendError::SegmentError("Read transaction does not have a chunk in it.".to_string())
        })?;
        Ok(data)
    }
}

#[derive(Debug)]
/// Analogue of `Segment` that uses a `BufWriter`, but can only allow write operations
pub struct WriteSegment<T: Write> {
    handle: BufWriter<T>,
    size_limit: u64,
}

impl<T: Write + Seek> WriteSegment<T> {
    /// Returns the size in bytes of the segment
    pub fn size(&mut self) -> u64 {
        self.handle.seek(SeekFrom::End(0)).unwrap()
    }

    pub fn write_chunk(&mut self, chunk: Chunk, id: ChunkID) -> Result<(u64, u64)> {
        let tx = Transaction::encode_insert(chunk, id);
        let start = self.handle.seek(SeekFrom::End(0))?;
        rmps::encode::write(&mut self.handle, &tx)?;
        let end = self.handle.seek(SeekFrom::End(0))?;
        let length = end - start;
        Ok((start, length))
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
        let cursor = Cursor::new(Vec::<u8>::new());
        let mut segment = Segment::new(cursor, 100).unwrap();

        assert!(segment.read_header().unwrap().validate())
    }
}
