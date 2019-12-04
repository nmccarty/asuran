use anyhow::Result;
use std::io::{Read, Write};
use uuid::Uuid;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Header {
    magic_number: [u8; 8],
    implementation_uuid: Uuid,
    version_bytes: [u8; 6],
}

impl Header {
    /// Creates a new segment header with correct values for this version of libasuran
    pub fn new() -> Header {
        Header {
            magic_number: b"ASURAN_S".clone(),
            implementation_uuid: crate::IMPLEMENTATION_UUID.clone(),
            version_bytes: crate::VERSION_BYTES.clone(),
        }
    }

    /// Serializes the header to a byte stream
    pub fn to_bytes(&self) -> [u8; 30] {
        let mut output = [0_u8; 30];
        let mut wrt: &mut [u8] = &mut output;
        wrt.write_all(&self.magic_number).unwrap();
        wrt.write_all(self.implementation_uuid.as_bytes()).unwrap();
        wrt.write_all(&self.version_bytes).unwrap();
        output
    }

    /// Reads a header from a reader
    pub fn from_read(mut bytes: impl Read) -> Result<Header> {
        let mut magic_number = [0_u8; 8];
        bytes.read_exact(&mut magic_number)?;
        let mut uuid_bytes = [0_u8; 16];
        bytes.read_exact(&mut uuid_bytes)?;
        let implementation_uuid = Uuid::from_bytes(uuid_bytes);
        let mut version_bytes = [0_u8; 6];
        bytes.read_exact(&mut version_bytes)?;
        Ok(Header {
            magic_number,
            implementation_uuid,
            version_bytes,
        })
    }

    /// Checks if a header is valid for this version of libasuran
    ///
    /// Currently only checks the header
    pub fn validate(&self) -> bool {
        &self.magic_number == b"ASURAN_S"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_sanity() {
        let input = Header::new();
        let bytes = input.to_bytes();
        let output = Header::from_read(&bytes[..]).unwrap();

        println!("{:02X?}", output);
        println!("{:02X?}", bytes);

        assert!(output.validate());
        assert_eq!(input, output);
    }
}
