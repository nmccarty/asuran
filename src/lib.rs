//! libasuran provides a consistent high-level interface to asuran archives
//! across multiple storage backends and backup targets.
//!
//! Asuran allows for the storing of multiple rich archives in a single repository,
//! while providing encryption, compression, and global deduplication.
use byteorder::{BigEndian, WriteBytesExt};
use lazy_static::lazy_static;
use uuid::Uuid;

pub mod chunker;
pub mod manifest;
pub mod repository;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

lazy_static! {
    /// The versions bytes for this version of libasuran, the concationation of major+minor+patch as
    /// u16s in network byte order
    pub static ref VERSION_BYTES: [u8; 6] = {
        let mut output = [0_u8;6];
        let items = VERSION.split('.').map(|x| x.parse::<u16>().unwrap()).collect::<Vec<_>>();
        assert!(items.len() == 3);
        let mut wrt: &mut[u8] = &mut output;
        for i in items.iter() {
            wrt.write_u16::<BigEndian>(*i).unwrap();
        }

        output
    };

    /// The UUID of this asuran implementation
    pub static ref IMPLEMNATION_UUID: Uuid = Uuid::parse_str("bfd30b79-4687-404e-a84d-112383994b26").unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use byteorder::ReadBytesExt;

    #[test]
    fn version_bytes_sanity() {
        let bytes: &[u8; 6] = &VERSION_BYTES;
        let mut bytes: &[u8] = bytes;
        let major = bytes.read_u16::<BigEndian>().unwrap();
        let minor = bytes.read_u16::<BigEndian>().unwrap();
        let patch = bytes.read_u16::<BigEndian>().unwrap();
        let version_string = format!("{}.{}.{}", major, minor, patch);
        println!("{:?}", &*VERSION_BYTES);
        println!("{}", version_string);
        assert_eq!(version_string, VERSION);
    }
}
