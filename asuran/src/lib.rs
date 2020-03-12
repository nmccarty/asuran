//! libasuran provides a consistent high-level interface to asuran archives
//! across multiple storage backends and backup targets.
//!
//! Asuran allows for the storing of multiple rich archives in a single repository,
//! while providing encryption, compression, and global deduplication.
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::must_use)]

use byteorder::{BigEndian, WriteBytesExt};
use lazy_static::lazy_static;
use semver::Version;
use std::convert::TryInto;
use uuid::Uuid;

pub mod chunker;
pub mod manifest;
pub mod repository;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

lazy_static! {
    /// The pieces of the version string for this version of libasuran
    pub static ref VERSION_PIECES: [u16; 3] = {
        let mut output = [0_u16; 3];
        let version = Version::parse(VERSION).expect("Unable to parse version");
        output[0] = version.major.try_into().expect("Major version too big");
        output[1] = version.minor.try_into().expect("Minor version too big");
        output[2] = version.patch.try_into().expect("Patch version too big");
        output
    };

    /// The versions bytes for this version of libasuran, the concationation of major+minor+patch as
    /// u16s in network byte order
    pub static ref VERSION_BYTES: [u8; 6] = {
        let mut output = [0_u8;6];
        let items = VERSION_PIECES.iter();
        assert!(items.len() == 3);
        let mut wrt: &mut[u8] = &mut output;
        for i in items {
            wrt.write_u16::<BigEndian>(*i).unwrap();
        }

        output
    };



    /// The UUID of this asuran implementation
    pub static ref IMPLEMENTATION_UUID: Uuid = Uuid::parse_str("bfd30b79-4687-404e-a84d-112383994b26").unwrap();
}
