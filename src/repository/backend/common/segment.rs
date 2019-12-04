use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::repository::backend::TransactionType;
use crate::repository::ChunkID;

const MAGIC_NUMBER: [u8; 8] = *b"ASURAN_S";

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

    pub fn uuid(&self) -> Uuid {
        Uuid::from_bytes(self.implementation_uuid)
    }

    pub fn version_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Default for Header {
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

    pub fn id(&self) -> ChunkID {
        self.id
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
