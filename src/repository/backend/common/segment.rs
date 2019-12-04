use crate::repository::backend::TransactionType;
use crate::repository::ChunkID;
use anyhow::{Context, Result};
use rmp_serde as rpms;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};
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
    pub fn new(handle: T, size_limit: u64) -> Segment<T> {
        Segment { handle, size_limit }
    }

    pub fn into_handle(self) -> SegmentHandle<T> {
        SegmentHandle {
            handle: Arc::new(Mutex::new(self)),
        }
    }
}

impl<T: Read + Write + Seek> crate::repository::backend::Segment for Segment<T> {
    fn free_bytes(&mut self) -> u64 {
        let end = self.handle.seek(SeekFrom::End(0)).unwrap();
        self.size_limit - end
    }
    fn read_chunk(&mut self, start: u64, _length: u64) -> Result<Vec<u8>> {
        self.handle.seek(SeekFrom::Start(start))?;
        let mut tx: Transaction = rpms::decode::from_read(&mut self.handle)?;
        let data = tx
            .take_data()
            .with_context(|| format!("Read transaction {:?} does not have a chunk in it.", tx))?;
        Ok(data)
    }
    fn write_chunk(&mut self, chunk: &[u8], id: ChunkID) -> Result<(u64, u64)> {
        let tx = Transaction::encode_insert(chunk.to_vec(), id);
        let start = self.handle.seek(SeekFrom::End(0))?;
        rpms::encode::write(&mut self.handle, &tx)?;
        let end = self.handle.seek(SeekFrom::End(0))?;
        let length = end - start;
        Ok((start, length))
    }
}

/// Generic Segment implemtation wrapping a Segment<T> in Arc<Mutex<>>
///
/// Arc and Mutex are both required, as some types we may wish to use, such as
/// `ssh2::File` are not thread safe, and the implementation needs to be general.
#[derive(Clone, Debug)]
pub struct SegmentHandle<T> {
    handle: Arc<Mutex<Segment<T>>>,
}

impl<T: Read + Write + Seek> crate::repository::backend::Segment for SegmentHandle<T> {
    fn free_bytes(&mut self) -> u64 {
        self.handle.lock().unwrap().free_bytes()
    }

    fn read_chunk(&mut self, start: u64, length: u64) -> Result<Vec<u8>> {
        self.handle.lock().unwrap().read_chunk(start, length)
    }

    fn write_chunk(&mut self, chunk: &[u8], id: ChunkID) -> Result<(u64, u64)> {
        self.handle.lock().unwrap().write_chunk(chunk, id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
