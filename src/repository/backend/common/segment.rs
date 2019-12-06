use crate::repository::backend::TransactionType;
use crate::repository::ChunkID;
use anyhow::{anyhow, Context, Result};
use async_std::sync;
use async_std::task::block_on;
use async_trait::async_trait;
use rmp_serde as rpms;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use uuid::Uuid;

/// Magic number used for asuran segment files
///
/// More or less completly arbitrary, but used to validate files
const MAGIC_NUMBER: [u8; 8] = *b"ASURAN_S";

/// Represenetation of the header at the start of each file
///
/// Designed to be bincoded directly into a spec compliant format with big endian set
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

/// Transaction wrapper struct
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Transaction {
    tx_type: TransactionType,
    id: ChunkID,
    chunk: Option<Vec<u8>>,
}

impl Transaction {
    pub fn transaction_type(&self) -> TransactionType {
        self.tx_type
    }

    pub fn encode_insert(input: Vec<u8>, id: ChunkID) -> Transaction {
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

    pub fn data(&self) -> Option<&[u8]> {
        self.chunk.as_ref().map(|x| &x[..])
    }

    pub fn take_data(&mut self) -> Option<Vec<u8>> {
        self.chunk.take()
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
                Err(anyhow!("Segment failed header validation"))
            }
        }
    }

    /// If the segment has zero length, will write the header and return Ok(true)
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
                .serialize_into(&mut self.handle, &header)?;
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
        let header: Header = config.big_endian().deserialize_from(&mut self.handle)?;
        Ok(header)
    }
}

#[async_trait]
impl<T: Read + Write + Seek + Send> crate::repository::backend::Segment for Segment<T> {
    fn free_bytes(&mut self) -> u64 {
        let end = self.handle.seek(SeekFrom::End(0)).unwrap();
        self.size_limit - end
    }
    #[allow(clippy::used_underscore_binding)]
    async fn read_chunk(&mut self, start: u64, _length: u64) -> Result<Vec<u8>> {
        self.handle.seek(SeekFrom::Start(start))?;
        let mut tx: Transaction = rpms::decode::from_read(&mut self.handle)?;
        let data = tx
            .take_data()
            .with_context(|| format!("Read transaction {:?} does not have a chunk in it.", tx))?;
        Ok(data)
    }
    async fn write_chunk(&mut self, chunk: &[u8], id: ChunkID) -> Result<(u64, u64)> {
        let tx = Transaction::encode_insert(chunk.to_vec(), id);
        let start = self.handle.seek(SeekFrom::End(0))?;
        rpms::encode::write(&mut self.handle, &tx)?;
        let end = self.handle.seek(SeekFrom::End(0))?;
        let length = end - start;
        Ok((start, length))
    }
}

/// Generic Segment implemtation wrapping a T in Arc<Mutex<>>
///
/// Arc and Mutex are both required, as some types we may wish to use, such as
/// `ssh2::File` are not thread safe, and the implementation needs to be general.
#[derive(Clone, Debug)]
pub struct SegmentHandle<T> {
    handle: Arc<sync::Mutex<T>>,
    size_limit: u64,
}

impl<T: Read + Write + Seek> SegmentHandle<T> {
    /// Creates a new segment given a reader and a maximum size
    pub async fn new(handle: T, size_limit: u64) -> Result<SegmentHandle<T>> {
        let mut s = SegmentHandle {
            handle: Arc::new(sync::Mutex::new(handle)),
            size_limit,
        };
        // Attempt to write the header
        let written = s.write_header().await?;
        if written {
            // Segment was empty, pass along as is
            Ok(s)
        } else {
            // Attempt to read the header
            let header = s.read_header().await?;
            // Validate it
            if header.validate() {
                Ok(s)
            } else {
                Err(anyhow!("Segment failed header validation"))
            }
        }
    }

    /// If the segment has zero length, will write the header and return Ok(true)
    ///
    /// If the segment has non-zero length, will do nothing and return Ok(false)
    ///
    /// An error in reading/writing will bubble up.
    pub async fn write_header(&mut self) -> Result<bool> {
        // Am i empty?
        let mut handle = self.handle.lock().await;
        let end = handle.seek(SeekFrom::End(0))?;
        if end == 0 {
            // If we are empty, then the handle is at the start of the file
            let header = Header::default();
            let mut config = bincode::config();
            config.big_endian().serialize_into(&mut *handle, &header)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Attempts to read the header from the Segment
    ///
    /// Will return error if the read fails
    pub async fn read_header(&mut self) -> Result<Header> {
        let mut handle = self.handle.lock().await;
        handle.seek(SeekFrom::Start(0))?;
        let mut config = bincode::config();
        let header: Header = config.big_endian().deserialize_from(&mut *handle)?;
        Ok(header)
    }
}

#[async_trait]
impl<T: Read + Write + Seek + Send> crate::repository::backend::Segment for SegmentHandle<T> {
    fn free_bytes(&mut self) -> u64 {
        let mut handle = block_on(self.handle.lock());
        let end = handle.seek(SeekFrom::End(0)).unwrap();
        self.size_limit - end
    }

    #[allow(clippy::used_underscore_binding)]
    async fn read_chunk(&mut self, start: u64, _length: u64) -> Result<Vec<u8>> {
        let mut handle = self.handle.lock().await;
        handle.seek(SeekFrom::Start(start))?;
        let mut tx: Transaction = rpms::decode::from_read(&mut *handle)?;
        let data = tx
            .take_data()
            .with_context(|| format!("Read transaction {:?} does not have a chunk in it.", tx))?;
        Ok(data)
    }
    async fn write_chunk(&mut self, chunk: &[u8], id: ChunkID) -> Result<(u64, u64)> {
        let mut handle = self.handle.lock().await;
        let tx = Transaction::encode_insert(chunk.to_vec(), id);
        let start = handle.seek(SeekFrom::End(0))?;
        rpms::encode::write(&mut *handle, &tx)?;
        let end = handle.seek(SeekFrom::End(0))?;
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
        assert_eq!(output.version_string(), crate::VERSION);
    }

    #[test]
    fn segment_header_sanity() {
        let cursor = Cursor::new(Vec::<u8>::new());
        let mut segment = Segment::new(cursor, 100).unwrap();

        assert!(segment.read_header().unwrap().validate())
    }

    #[test]
    fn segment_handle_header_sanity() {
        let cursor = Cursor::new(Vec::<u8>::new());
        let mut segment = block_on(SegmentHandle::new(cursor, 100)).unwrap();

        assert!(block_on(segment.read_header()).unwrap().validate())
    }
}
