/*!
This module contains data structures describing components of the `FlatFile`
on-disk representation.
*/
use crate::repository::{Chunk, ChunkHeader, ChunkID, ChunkSettings, EncryptedKey, Key};

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use chrono::{DateTime, FixedOffset};
use rmp_serde as rmps;
use semver::Version;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use std::collections::HashMap;
use std::convert::TryInto;
use std::io::{Read, Write};

pub const MAGIC_NUMBER: [u8; 8] = *b"ASURAN_F";

/// An error for things that go wrong with interacting with flatfile transactions and headers
#[derive(Error, Debug)]
pub enum FlatFileError {
    #[error("General I/O Error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("Configuration Encode Error: {0}")]
    Encode(#[from] rmps::encode::Error),
    #[error("Configuration Decode Error: {0}")]
    Decode(#[from] rmps::decode::Error),
    #[error("Unable to encode key in u16::MAX bytes")]
    KeyTooLong,
    #[error("Magic number was not correct for Asuran FlatFile format")]
    InvalidMagicNumber,
    #[error("Semver component {0} too high: {1}")]
    SemverToHigh(u64, Version),
    #[error("Chunk decryption failed: {0}")]
    ChunkError(#[from] crate::repository::chunk::ChunkError),
}

type Result<T> = std::result::Result<T, FlatFileError>;

/// A struct representation of the Asuran `FlatFile` global header.
///
/// The initial/global header contains three components:
///
/// 1. Magic Number
///
///     The magic number identifying asuran `FlatFile`s is the 8-byte string
///     `b"ASURAN_F"`.
///
/// 2. Length of header
///
///     The total length of the encrypted key, in bytes, as a u16.
///
/// 3. The `EncryptedKey`
///
///     The serialized, encrypted key material for this repository.
///
/// The first byte of the first entry immediately follows the last byte of the
/// initial header
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatFileHeader {
    pub magic_number: [u8; 8],
    pub length: u16,
    pub enc_key: Vec<u8>,
}

impl FlatFileHeader {
    /// Creates a new `FlatFile` header from an encrypted key.
    ///
    /// # Errors
    ///
    /// Will return `Err(FlatFileHeaderError::KeyTooLong)` if the key is unable to be
    /// serialized in `u16::MAX` (65,535) bytes. This (realistically) should never happen.
    pub fn new(key: &EncryptedKey) -> Result<FlatFileHeader> {
        let enc_key = rmps::encode::to_vec(key).expect(
            "Encrypted key does not have any types that should fail to serialize.\
             This should never fail.",
        );
        let length: u16 = enc_key
            .len()
            .try_into()
            .map_err(|_| FlatFileError::KeyTooLong)?;

        Ok(FlatFileHeader {
            magic_number: MAGIC_NUMBER,
            length,
            enc_key,
        })
    }

    /// Verifies the magic number in this header against the defined magic number for
    /// Asuran `FlatFile`s.
    ///
    /// Returns true if the magic number is correct.
    pub fn verify_magic_number(&self) -> bool {
        self.magic_number == MAGIC_NUMBER
    }

    /// Decodes the contained `EncryptedKey`
    pub fn key(&self) -> Result<EncryptedKey> {
        let enc_key = rmps::decode::from_slice(&self.enc_key[..])?;
        Ok(enc_key)
    }

    /// Reads the global header from an Asuran `FlatFile`.
    ///
    /// The passed in Read must be seeked to the start of the file.
    ///
    /// # Errors
    ///
    /// Will return `Err(InvalidMagicNumber)` if the magic number of the header is not
    /// correct for the `FlatFile` format
    ///
    /// Will also return `Err` if there is an underlying I/O error.
    pub fn from_read(mut read: impl Read) -> Result<FlatFileHeader> {
        let mut magic_number = [0_u8; 8];
        read.read_exact(&mut magic_number)?;
        let length: u16 = read.read_u16::<NetworkEndian>()?;
        let mut enc_key = vec![0_u8; length as usize];
        read.read_exact(&mut enc_key[..])?;
        let header = FlatFileHeader {
            magic_number,
            length,
            enc_key,
        };
        if !header.verify_magic_number() {
            return Err(FlatFileError::InvalidMagicNumber);
        }
        Ok(header)
    }

    /// Writes the Asuran `FlatFile` Header to the given `Write`
    ///
    /// The provided `Write` must be seeked to the start of the file.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an underlying I/O error.
    pub fn to_write(&self, mut write: impl Write) -> Result<()> {
        write.write_all(&MAGIC_NUMBER)?;
        write.write_u16::<NetworkEndian>(self.length)?;
        write.write_all(&self.enc_key[..])?;
        Ok(())
    }

    /// Returns the total length (in bytes) of this Header
    pub fn total_length(&self) -> u64 {
        // This is the length of the encrypted key, plus 2 bytes for the length u16, and 8 bytes for
        // the magic number.
        u64::from(self.length) + 10
    }
}

/// A struct representation of the header portion of an entry.
///
/// An entry header is a sequence of 3 `u16`s, followed by two `u64`s, and then a
/// 16-byte UUID. In order they
/// are:
///
/// 1. The major version of the version of `asuran` writing to this Repository.
/// 2. The minor version of the version of `asuran` writing to this Repository.
/// 3. The patch version of the version of `asuran` writing to this Repository.
/// 4. The offset in the file of the footer for this entry
/// 5. The offset in the file of the header for the next entry
/// 6. The implementation UUID of the Asuran implementation writing to this repository
///
/// This will typically be initially written to the file with the `footer_offset`
/// and `next_header_offset` as 0, and then be updated when writing is closed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EntryHeader {
    pub semver_major: u16,
    pub semver_minor: u16,
    pub semver_patch: u16,
    pub footer_offset: u64,
    pub next_header_offset: u64,
    pub uuid_bytes: [u8; 16],
}

impl EntryHeader {
    /// Creates a new `EntryHeader` with the given information.
    ///
    /// # Errors
    ///
    /// Will return `Err(SemverToLarge)` if a semver with any version field greater than
    /// `u16::MAX` (65,535) is passed.
    pub fn new(
        version: &Version,
        footer_offset: u64,
        next_header_offset: u64,
        uuid: Uuid,
    ) -> Result<EntryHeader> {
        let semver_major: u16 = version
            .major
            .try_into()
            .map_err(|_| FlatFileError::SemverToHigh(version.major, version.clone()))?;
        let semver_minor: u16 = version
            .minor
            .try_into()
            .map_err(|_| FlatFileError::SemverToHigh(version.minor, version.clone()))?;
        let semver_patch: u16 = version
            .patch
            .try_into()
            .map_err(|_| FlatFileError::SemverToHigh(version.patch, version.clone()))?;
        Ok(EntryHeader {
            semver_major,
            semver_minor,
            semver_patch,
            footer_offset,
            next_header_offset,
            uuid_bytes: *uuid.as_bytes(),
        })
    }

    /// Returns the semver `Version` packed within this `EntryHeader`
    pub fn version(&self) -> Version {
        Version::new(
            u64::from(self.semver_major),
            u64::from(self.semver_minor),
            u64::from(self.semver_patch),
        )
    }

    /// Returns the implementation UUID packed within this `EntryHeader`
    pub fn uuid(&self) -> Uuid {
        Uuid::from_bytes(self.uuid_bytes)
    }

    /// Reads an `EntryHeader` from the provided `Read`
    ///
    /// The provided `Read` must be seeked to the start of the `EntryHeader`.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an underlying I/O error.
    pub fn from_read(mut read: impl Read) -> Result<EntryHeader> {
        let semver_major = read.read_u16::<NetworkEndian>()?;
        let semver_minor = read.read_u16::<NetworkEndian>()?;
        let semver_patch = read.read_u16::<NetworkEndian>()?;
        let footer_offset = read.read_u64::<NetworkEndian>()?;
        let next_header_offset = read.read_u64::<NetworkEndian>()?;
        let mut uuid_bytes = [0_u8; 16];
        read.read_exact(&mut uuid_bytes[..])?;

        Ok(EntryHeader {
            semver_major,
            semver_minor,
            semver_patch,
            footer_offset,
            next_header_offset,
            uuid_bytes,
        })
    }

    /// Writes this `EntryHeader` to the provided `Write`
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an underlying I/O error
    pub fn to_write(&self, mut write: impl Write) -> Result<()> {
        write.write_u16::<NetworkEndian>(self.semver_major)?;
        write.write_u16::<NetworkEndian>(self.semver_minor)?;
        write.write_u16::<NetworkEndian>(self.semver_patch)?;
        write.write_u64::<NetworkEndian>(self.footer_offset)?;
        write.write_u64::<NetworkEndian>(self.next_header_offset)?;
        write.write_all(&self.uuid_bytes[..])?;
        Ok(())
    }
}

/// A struct representation of the repository metadata associated with this entry.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryFooterData {
    /// The locations of the `Chunk`s added by this entry.
    ///
    /// Encoded as (id, starting offset, length) tuples.
    pub chunk_locations: Vec<(ChunkID, u64, u64)>,
    /// The headers of the `Chunk`s added by this entry.
    pub chunk_headers: HashMap<ChunkID, ChunkHeader>,
    /// The `ChunkID`s of the archive's added by this entry.
    pub archives: Vec<(ChunkID, DateTime<FixedOffset>)>,
    /// The current default `ChunkSettings` of this repository
    pub chunk_settings: ChunkSettings,
}

impl EntryFooterData {
    /// Creates a new, empty, `EntryFooterData`
    pub fn new(chunk_settings: ChunkSettings) -> EntryFooterData {
        EntryFooterData {
            chunk_locations: Vec::new(),
            archives: Vec::new(),
            chunk_settings,
            chunk_headers: HashMap::new(),
        }
    }
    /// Adds a chunk to the `chunk_locations` list
    pub fn add_chunk(&mut self, id: ChunkID, location: u64, length: u64) {
        self.chunk_locations.push((id, location, length));
    }
    /// Adds a header to the `chunk_headers` map
    pub fn add_header(&mut self, id: ChunkID, header: ChunkHeader) {
        self.chunk_headers.insert(id, header);
    }
    /// Adds an archive to the `archives` list
    pub fn add_archive(&mut self, id: ChunkID, timestamp: DateTime<FixedOffset>) {
        self.archives.push((id, timestamp))
    }
    /// Returns true if any of the internal structures have data in them
    pub fn dirty(&self) -> bool {
        !self.chunk_locations.is_empty()
            || !self.chunk_headers.is_empty()
            || !self.archives.is_empty()
    }
}

pub struct EntryFooter {
    /// The length, in bytes, of the following `Chunk`
    chunk_bytes: Vec<u8>,
}

impl EntryFooter {
    /// Encodes an `EntryFooterData` into an `EntryFooter`, encrypting/compressing with
    /// the provided chunk settings and key.
    pub fn from_data(
        data: &EntryFooterData,
        key: &Key,
        chunk_settings: ChunkSettings,
    ) -> EntryFooter {
        let data = rmps::encode::to_vec(data).expect(
            "EntryFooterData contains no types for which serialization can fail.\
             This should, realistically, never happen.",
        );
        let chunk = Chunk::pack(
            data,
            chunk_settings.compression,
            chunk_settings.encryption,
            chunk_settings.hmac,
            key,
        );
        let chunk_bytes = rmps::encode::to_vec(&chunk).expect(
            "Chunk contains no types for which serialization can fail.\
             This should, realistically, never happen.",
        );
        EntryFooter { chunk_bytes }
    }

    /// Unpacks the interior `Chunk` into an `EntryFooterData` struct.
    ///
    /// # Errors
    ///
    /// - If decoding the `Chunk` from the interior bytes fails
    /// - If decrypting/decompressing the `Chunk` fails
    /// - If decoding the `EntryFooterData` from the unpacked bytes fails
    pub fn into_data(self, key: &Key) -> Result<EntryFooterData> {
        let chunk: Chunk = rmps::decode::from_slice(&self.chunk_bytes[..])?;
        let bytes = chunk.unpack(key)?;
        let data: EntryFooterData = rmps::decode::from_slice(&bytes[..])?;
        Ok(data)
    }

    /// Decodes an `EntryFooter` from the provided `Read`.
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an underlying I/O error.
    pub fn from_read(mut read: impl Read) -> Result<EntryFooter> {
        let length = read.read_u64::<NetworkEndian>()?;
        let buffer_len: usize = length
            .try_into()
            .expect("EntryFooter chunk too large to possibly fit in memory.");
        let mut chunk_bytes = vec![0_u8; buffer_len];
        read.read_exact(&mut chunk_bytes[..])?;
        Ok(EntryFooter { chunk_bytes })
    }

    /// Encodes an `EntryFooter` to the provided `Write`
    ///
    /// # Errors
    ///
    /// Will return `Err` if there is an underlying I/O
    pub fn to_write(&self, mut write: impl Write) -> Result<()> {
        write.write_u64::<NetworkEndian>(self.chunk_bytes.len() as u64)?;
        write.write_all(&self.chunk_bytes[..])?;
        Ok(())
    }
}
