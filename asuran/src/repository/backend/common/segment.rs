use crate::repository::backend::{BackendError, Result, SegmentDescriptor, TransactionType};
use crate::repository::{Chunk, ChunkID};
use futures::channel;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use rmp_serde as rpms;
use serde::{Deserialize, Serialize};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use tokio::task;
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
///
/// TODO: Document this better
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Transaction {
    tx_type: TransactionType,
    id: ChunkID,
    /// Conceptually, this should be an Option<Vec<u8>>. We instead employ an optimization where we use
    /// a vector with a zero length to simualte the none case. This has the side effect of requiring that
    /// all chunk payloads be 1 byte or longer, but in practice this is not a serious issue as the content
    /// will always be packed chunk structs, which will always have a length greater than zero, as they
    /// contain manditory tags in addition to data.
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

    async fn free_bytes(&mut self) -> u64 {
        let end = self.handle.seek(SeekFrom::End(0)).unwrap();
        self.size_limit - end
    }

    pub fn read_chunk(&mut self, start: u64, _length: u64) -> Result<Chunk> {
        self.handle.seek(SeekFrom::Start(start))?;
        let tx: Transaction = rpms::decode::from_read(&mut self.handle)?;
        let data = tx.take_data().ok_or_else(|| {
            BackendError::SegmentError("Read transaction does not have a chunk in it.".to_string())
        })?;
        Ok(data)
    }

    pub fn write_chunk(&mut self, chunk: Chunk, id: ChunkID) -> Result<(u64, u64)> {
        let tx = Transaction::encode_insert(chunk, id);
        let start = self.handle.seek(SeekFrom::End(0))?;
        rpms::encode::write(&mut self.handle, &tx)?;
        let end = self.handle.seek(SeekFrom::End(0))?;
        let length = end - start;
        Ok((start, length))
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
pub struct ReadSegment<T> {
    handle: BufReader<T>,
    size_limit: u64,
}

impl<T: Read + Seek> ReadSegment<T> {
    pub fn read_chunk(&mut self, start: u64, _length: u64) -> Result<Chunk> {
        self.handle.seek(SeekFrom::Start(start))?;
        let tx: Transaction = rpms::decode::from_read(&mut self.handle)?;
        let data = tx.take_data().ok_or_else(|| {
            BackendError::SegmentError("Read transaction does not have a chunk in it.".to_string())
        })?;
        Ok(data)
    }
}

#[derive(Debug)]
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
        rpms::encode::write(&mut self.handle, &tx)?;
        let end = self.handle.seek(SeekFrom::End(0))?;
        let length = end - start;
        Ok((start, length))
    }
}

#[derive(Debug)]
pub struct SegmentStats {
    /// The used space in this segment
    pub size: u64,
    /// Number of bytes left in the segment before it hits quota
    pub free: u64,
    /// Quota of this segment
    pub quota: u64,
}

/// Describes a command that can be run on a segment
#[derive(Debug)]
pub enum SegmentCommand {
    Write(
        Chunk,
        ChunkID,
        channel::oneshot::Sender<Result<SegmentDescriptor>>,
    ),
    Read(SegmentDescriptor, channel::oneshot::Sender<Result<Chunk>>),
    Stats(channel::oneshot::Sender<SegmentStats>),
    Close(channel::oneshot::Sender<()>),
}

#[derive(Clone, Debug)]
pub struct TaskedSegment<R> {
    command_tx: channel::mpsc::Sender<SegmentCommand>,
    phantom: PhantomData<R>,
}

impl<R: Read + Write + Seek + Send + 'static> TaskedSegment<R> {
    pub fn new(reader: R, size_limit: u64, segment_id: u64) -> TaskedSegment<R> {
        let (tx, mut rx) = channel::mpsc::channel(100);
        task::spawn(async move {
            let mut segment = Segment::new(reader, size_limit).unwrap();
            let mut final_ret = None;
            while let Some(command) = rx.next().await {
                match command {
                    SegmentCommand::Write(data, id, ret) => {
                        let res = segment.write_chunk(data, id);
                        let out = res.map(|(start, _)| SegmentDescriptor { segment_id, start });
                        ret.send(out).unwrap();
                    }
                    SegmentCommand::Read(location, ret) => {
                        let chunk = segment.read_chunk(location.start, 0);
                        ret.send(chunk).unwrap();
                    }
                    SegmentCommand::Stats(ret) => {
                        let size = segment.size();
                        let free = segment.free_bytes().await;
                        let quota = segment.size_limit;
                        let stats = SegmentStats { size, free, quota };
                        ret.send(stats).unwrap();
                    }
                    SegmentCommand::Close(ret) => {
                        final_ret = Some(ret);
                        break;
                    }
                }
            }
            // Ensure that our internals are dropped before returning
            std::mem::drop(segment);
            std::mem::drop(rx);
            if let Some(ret) = final_ret {
                ret.send(()).unwrap();
            };
        });
        TaskedSegment {
            command_tx: tx,
            phantom: PhantomData,
        }
    }

    pub async fn write_chunk(&mut self, chunk: Chunk, id: ChunkID) -> Result<SegmentDescriptor> {
        let (tx, rx) = channel::oneshot::channel();
        self.command_tx
            .send(SegmentCommand::Write(chunk, id, tx))
            .await
            .unwrap();
        rx.await?
    }

    pub async fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        let (tx, rx) = channel::oneshot::channel();
        self.command_tx
            .send(SegmentCommand::Read(location, tx))
            .await
            .unwrap();
        rx.await?
    }

    pub async fn stats(&mut self) -> channel::oneshot::Receiver<SegmentStats> {
        let (tx, rx) = channel::oneshot::channel();
        self.command_tx
            .send(SegmentCommand::Stats(tx))
            .await
            .unwrap();
        rx
    }

    pub async fn close(&mut self) {
        let (tx, rx) = channel::oneshot::channel();
        self.command_tx
            .send(SegmentCommand::Close(tx))
            .await
            .unwrap();
        rx.await.unwrap();
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

}
