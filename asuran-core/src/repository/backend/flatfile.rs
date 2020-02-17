use crate::repository::{ChunkID, ChunkSettings, EncryptedKey};

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use chrono::prelude::*;
use rmp_serde as rmps;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use thiserror::Error;
use uuid::Uuid;

/// An error for things that go wrong with interacting with flatfile transactions and headers
#[derive(Error, Debug)]
pub enum FlatFileHeaderError {
    #[error("General I/O Error")]
    IOError(#[from] std::io::Error),
    #[error("Configuration Encode Error")]
    Encode(#[from] rmps::encode::Error),
    #[error("Configuration Decode Error")]
    Decode(#[from] rmps::decode::Error),
}

/// The header used by flatfile repositories
#[derive(Serialize, Deserialize, Debug)]
pub struct Header {
    pub magic_number: [u8; 8],
    pub implementation_uuid: [u8; 16],
    pub semver_major: u16,
    pub semver_minor: u16,
    pub semver_patch: u16,
    pub configuration: Configuration,
}

impl Header {
    /// Writes the header to the given Write
    ///
    /// Semver versions components are serialized in network order
    pub fn serialize(&self, write: &mut impl Write) -> Result<(), FlatFileHeaderError> {
        write.write_all(&self.magic_number)?;
        write.write_all(&self.implementation_uuid)?;
        write.write_u16::<NetworkEndian>(self.semver_major)?;
        write.write_u16::<NetworkEndian>(self.semver_minor)?;
        write.write_u16::<NetworkEndian>(self.semver_patch)?;
        rmps::encode::write(write, &self.configuration)?;
        Ok(())
    }

    /// Attempts to deserialize a header from the given Read
    pub fn deserialize(read: &mut impl Read) -> Result<Header, FlatFileHeaderError> {
        let mut magic_number = [0_u8; 8];
        read.read_exact(&mut magic_number)?;
        let mut implementation_uuid = [0_u8; 16];
        read.read_exact(&mut implementation_uuid)?;
        let semver_major = read.read_u16::<NetworkEndian>()?;
        let semver_minor = read.read_u16::<NetworkEndian>()?;
        let semver_patch = read.read_u16::<NetworkEndian>()?;
        let configuration = rmps::decode::from_read(read)?;
        Ok(Header {
            magic_number,
            implementation_uuid,
            semver_major,
            semver_minor,
            semver_patch,
            configuration,
        })
    }

    // Gets the UUID of the implementation
    pub fn implementation_uuid(&self) -> Uuid {
        Uuid::from_bytes(self.implementation_uuid)
    }

    // Returns the semver version associated with this header
    pub fn semver(&self) -> Version {
        Version::new(
            u64::from(self.semver_major),
            u64::from(self.semver_minor),
            u64::from(self.semver_patch),
        )
    }
}

/// The configuration struct used by flatfile repositories
#[derive(Serialize, Deserialize, Debug)]
pub struct Configuration {
    pub key: EncryptedKey,
    pub chunk_settings: ChunkSettings,
}

/// Represents the various types of transaction entries in a flatfile repository
#[derive(Serialize, Deserialize)]
pub enum FlatFileTransaction {
    /// An insertion of a data chunk into the repository
    Insert { id: ChunkID, chunk: Vec<u8> },
    /// A deletion of a data chunk from the repository
    Delete { id: ChunkID },
    /// An insertion of an `Archive` into the repository
    ///
    /// The Serialized `Archive` is stored as a `Chunk` containing the serialized Arhive
    ManifestInsert {
        id: ChunkID,
        name: String,
        timestamp: DateTime<FixedOffset>,
    },
}
