use crate::repository::{ChunkID, Key, HMAC};

use chrono::prelude::*;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use serde_cbor as cbor;

/// Wrapper around [u8; 32] used for transaction hashes
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Hash)]
pub struct ManifestID([u8; 32]);

/// Describes a transaction in a manifest
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ManifestTransaction {
    /// The HMACs of all previous branch heads in the repository that this transaction references
    previous_heads: Vec<ManifestID>,
    /// The location of the archive this trasnaction refrences within the archive
    pointer: ChunkID,
    /// The timestamp of this Transactions Creation
    timestamp: DateTime<FixedOffset>,
    /// A 128-bit random nonce
    ///
    /// This is canonically stored as an array of bytes, to keep the serializer and
    /// deserializer simple, while preventing issues with other platforms who may not
    /// have support for the exact same integer types as rust
    ///
    /// This value is used for ensuring uniqueness when constructing the Manifest
    /// Merkle Tree
    nonce: [u8; 16],
    /// The type of HMAC used for this transaction
    hmac: HMAC,
    /// The HMAC tag of this transaction
    ///
    /// This is calculated based off the compact (array form) messagepacked encoding of
    /// this struct with this value set to all zeros
    tag: ManifestID,
}

impl ManifestTransaction {
    /// Constructs a new `ManifestTransaction` from the given list of previous heads, a
    /// pointer, a name, a timestamp, and an HMAC method to use
    ///
    /// Will automatically produce the random nonce, and update the tag
    pub fn new(
        previous_heads: &[ManifestID],
        pointer: ChunkID,
        timestamp: DateTime<FixedOffset>,
        hmac: HMAC,
        key: &Key,
    ) -> ManifestTransaction {
        let mut nonce = [0_u8; 16];
        rand::thread_rng().fill_bytes(&mut nonce);
        let mut tx = ManifestTransaction {
            previous_heads: previous_heads.to_vec(),
            pointer,
            timestamp,
            nonce,
            hmac,
            tag: ManifestID([0_u8; 32]),
        };
        tx.update_tag(key);
        tx
    }

    /// Serializes the struct, performs the HMAC, and updates the value in place
    ///
    /// Will zero the hmac value before performing the operation
    fn update_tag(&mut self, key: &Key) {
        self.tag.0 = [0_u8; 32];
        let bytes = cbor::ser::to_vec(self).expect("Serialization in hmac failed");
        let tag = self.hmac.mac(&bytes[..], key);
        self.tag.0.copy_from_slice(&tag[..32]);
    }

    /// Returns a refrence to the list of previous heads
    pub fn previous_heads(&self) -> &[ManifestID] {
        &self.previous_heads[..]
    }

    /// Returns the pointer to the archive in the repository
    pub fn pointer(&self) -> ChunkID {
        self.pointer
    }

    /// Returns the timestamp of the archive
    pub fn timestamp(&self) -> DateTime<FixedOffset> {
        self.timestamp
    }

    /// Returns the HMAC value tag of this transaction
    pub fn tag(&self) -> ManifestID {
        self.tag
    }

    /// Verifies the hmac of the transaction
    ///
    /// This does not descend down the DAG, will only verfiy thistransaction.
    pub fn verify(&self, key: &Key) -> bool {
        let tag = &self.tag;
        let mut copy = self.clone();
        copy.update_tag(key);
        tag == &copy.tag
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_tx(_name: &str, key: &Key) -> ManifestTransaction {
        let hmac = HMAC::Blake2b;
        let pointer = ChunkID::new(&[1_u8; 32]);
        let timestamp = Local::now().with_timezone(Local::now().offset());
        ManifestTransaction::new(&[], pointer, timestamp, hmac, key)
    }

    // Creating a manifest and verifying it should result in success
    #[test]
    fn create_and_verify() {
        let key = Key::random(32);
        let tx = create_tx("test", &key);
        assert!(tx.verify(&key));
    }

    // Modifying the content of a transaction without updating its tag should cause it to fail
    #[test]
    #[should_panic]
    fn modify_verify() {
        let key = Key::random(32);
        let mut tx = create_tx("test", &key);
        tx.previous_heads = vec![ManifestID([2_u8; 32])];
        assert!(tx.verify(&key));
    }

    // Verifying with the wrong key fails
    #[test]
    #[should_panic]
    fn verify_wrong_key() {
        let key = Key::random(32);
        let tx = create_tx("test", &key);
        let bad_key = Key::random(32);
        assert!(tx.verify(&bad_key));
    }

    // Serialize and deserilizing should still result in a valid tx
    #[test]
    fn serialize_deserialize() {
        let key = Key::random(32);
        let tx = create_tx("test", &key);
        let bytes = cbor::ser::to_vec(&tx).unwrap();
        let output_tx: ManifestTransaction = cbor::de::from_slice(&bytes[..]).unwrap();
        assert!(output_tx.verify(&key));
    }
}
