use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::cmp;
use std::collections::HashMap;

use crate::repository::backend::*;
use crate::repository::compression::*;
use crate::repository::encryption::*;
use crate::repository::hmac::*;

pub mod backend;
pub mod compression;
pub mod encryption;
pub mod hmac;

pub struct Repository {
    backend: Box<dyn Backend>,
    index: HashMap<Key, (u64, u64, u64)>,
    /// Default compression for new chunks
    compression: Compression,
    /// Default MAC algorthim for new chunks
    hmac: HMAC,
    /// Default encryption algorthim for new chunks
    encryption: Encryption,
    /// Encryption key for this repo
    key: Vec<u8>,
}

impl Repository {
    /// Creates a new repository with the specificed backend and defaults
    pub fn new(
        backend: Box<dyn Backend>,
        compression: Compression,
        hmac: HMAC,
        encryption: Encryption,
        key: &[u8],
    ) -> Repository {
        // Check for index, create a new one if it doesnt exist
        let index_vec = backend.get_index();
        let index = if index_vec.is_empty() {
            HashMap::new()
        } else {
            let mut de = Deserializer::new(index_vec.as_slice());
            Deserialize::deserialize(&mut de).expect("Unable to parse index")
        };
        let key = key.to_vec();

        Repository {
            backend,
            index,
            compression,
            hmac,
            encryption,
            key,
        }
    }

    /// Commits the index
    pub fn commit_index(&self) {
        let mut buff = Vec::<u8>::new();
        let mut se = Serializer::new(&mut buff);
        self.index.serialize(&mut se).unwrap();
        self.backend
            .write_index(&buff)
            .expect("Unable to commit index");
    }

    /// Writes a chunk to the repo
    ///
    /// Uses all defaults
    ///
    /// Will return None if writing the chunk fails
    pub fn write_chunk(&mut self, data: &[u8]) -> Option<Key> {
        let chunk = Chunk::pack(
            data,
            self.compression,
            self.encryption.new_iv(),
            self.hmac,
            &self.key,
        );
        let id = chunk.get_id();

        let mut buff = Vec::<u8>::new();
        chunk.serialize(&mut Serializer::new(&mut buff)).unwrap();

        // Get highest segment and check to see if has enough space
        let backend = &self.backend;
        let mut seg_id = backend.highest_segment();
        let test_segment = backend.get_segment(seg_id);
        // If no segments exist, we must create one
        let test_segment = if test_segment.is_none() {
            seg_id = backend.make_segment()?;
            backend.get_segment(seg_id)?
        } else {
            test_segment?
        };
        let mut segment = if test_segment.free_bytes() <= buff.len() as u64 {
            seg_id = backend.make_segment()?;
            backend.get_segment(seg_id)?
        } else {
            test_segment
        };

        let (start, length) = segment.write_chunk(&buff)?;
        self.index.insert(id, (seg_id, start, length));

        Some(id)
    }

    /// Determines if a chunk exists in the index
    pub fn has_chunk(&self, id: Key) -> bool {
        self.index.contains_key(&id)
    }

    /// Reads a chunk from the repo
    ///
    /// Returns none if reading the chunk fails
    pub fn read_chunk(&self, id: Key) -> Option<Vec<u8>> {
        // First, check if the chunk exists
        if self.has_chunk(id) {
            let (seg_id, start, length) = *self.index.get(&id)?;
            let mut segment = self.backend.get_segment(seg_id)?;
            let chunk_bytes = segment.read_chunk(start, length)?;

            let mut de = Deserializer::new(&chunk_bytes[..]);
            let chunk: Chunk = Deserialize::deserialize(&mut de).unwrap();

            let data = chunk.unpack(&self.key)?;

            Some(data)
        } else {
            None
        }
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        self.commit_index();
    }
}

/// Key for an object in a repository
#[derive(PartialEq, Eq, Copy, Clone, Serialize, Deserialize, Hash)]
pub struct Key {
    /// Keys are a bytestring of length 32
    ///
    /// This lines up well with SHA256 and other 256 bit hashes.
    /// Longer hashes will be truncated and shorter ones (not reccomended) will be padded.
    key: [u8; 32],
}

impl Key {
    /// Will create a new key from a slice.
    ///
    /// Keys longer than 32 bytes will be truncated.
    /// Keys shorter than 32 bytes will be padded at the end with zeros.
    pub fn new(input_key: &[u8]) -> Key {
        let mut key: [u8; 32] = [0; 32];
        key[..cmp::min(32, input_key.len())]
            .clone_from_slice(&input_key[..cmp::min(32, input_key.len())]);
        Key { key }
    }

    /// Returns an immutable refrence to the key in bytestring form
    pub fn get_key(&self) -> &[u8] {
        &self.key
    }
}

/// Data chunk
///
/// Encrypted, compressed object, to be stored in the repository
#[derive(Serialize, Deserialize)]
pub struct Chunk {
    /// The data of the chunk, stored as a vec of raw bytes
    #[serde(with = "serde_bytes")]
    data: Vec<u8>,
    /// Compression algorithim used
    compression: Compression,
    /// Encryption Algorithim used, also stores IV
    encryption: Encryption,
    /// HMAC algorithim used
    ///
    /// HAMC key is also the same as the repo encryption key
    hmac: HMAC,
    /// Actual MAC value of this chunk
    #[serde(with = "serde_bytes")]
    mac: Vec<u8>,
    /// Chunk ID, generated from the HMAC
    id: Key,
}

impl Chunk {
    /// Will Pack the data into a chunk with the given compression and encryption
    pub fn pack(
        data: &[u8],
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        key: &[u8],
    ) -> Chunk {
        let mac = hmac.mac(&data, key);
        let compressed_data = compression.compress(data);
        let data = encryption.encrypt(&compressed_data, key);
        let id = Key::new(&mac);
        Chunk {
            data,
            compression,
            encryption,
            hmac,
            mac,
            id,
        }
    }

    /// Decrypts and decompresses the data in the chunk
    ///
    /// Will return none if either the decompression or the decryption fail
    ///
    /// Will also return none if the HMAC verification fails
    pub fn unpack(&self, key: &[u8]) -> Option<Vec<u8>> {
        let decrypted_data = self.encryption.decrypt(&self.data, key)?;
        let decompressed_data = self.compression.decompress(&decrypted_data)?;

        if self.hmac.verify(&self.mac, &decompressed_data, key) {
            Some(decompressed_data)
        } else {
            None
        }
    }

    /// Creates a chunk from a raw bytestring with the given compressor
    /// and encryption algorithim
    pub fn from_bytes(
        data: &[u8],
        compression: Compression,
        encryption: Encryption,
        hmac: HMAC,
        mac: &[u8],
        id: Key,
    ) -> Chunk {
        Chunk {
            data: data.to_vec(),
            compression,
            encryption,
            hmac,
            mac: mac.to_vec(),
            id,
        }
    }

    /// Returns the length of the data bytes
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Determine if this chunk is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Returns a reference to the raw bytes of this chunk
    pub fn get_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Gets the key for this block
    pub fn get_id(&self) -> Key {
        self.id
    }

    #[cfg(test)]
    /// Testing only function used to corrupt the data
    pub fn break_data(&mut self, index: usize) {
        let val = self.data[index];
        if val == 0 {
            self.data[index] = 1;
        } else {
            self.data[index] = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::backend::filesystem::*;
    use crate::repository::backend::*;
    use rand::prelude::*;
    use std::str;
    use tempfile::tempdir;

    #[test]
    fn chunk_aes256cbc_zstd6() {
        let data_string =
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";

        let data_bytes = data_string.as_bytes();
        println!("Data: \n:{:X?}", data_bytes);
        let compression = Compression::ZStd { level: 6 };
        let encryption = Encryption::new_aes256cbc();
        let hmac = HMAC::SHA256;

        let mut key: [u8; 32] = [0; 32];
        thread_rng().fill_bytes(&mut key);

        let packed = Chunk::pack(&data_bytes, compression, encryption, hmac, &key);

        let output_bytes = packed.unpack(&key);

        assert_eq!(Some(data_string.as_bytes().to_vec()), output_bytes);
    }

    #[test]
    fn detect_bad_data() {
        let data_string = "I am but a humble test string";
        let data_bytes = data_string.as_bytes();
        let compression = Compression::NoCompression;
        let encryption = Encryption::NoEncryption;
        let hmac = HMAC::SHA256;

        let mut key: [u8; 32] = [0; 32];
        thread_rng().fill_bytes(&mut key);

        let mut packed = Chunk::pack(&data_bytes, compression, encryption, hmac, &key);
        packed.break_data(5);

        let result = packed.unpack(&key);

        assert_eq!(result, None);
    }

    #[test]
    fn repository_add_read() {
        let mut key: [u8; 32] = [0; 32];
        thread_rng().fill_bytes(&mut key);

        let size = 7 * 10_u64.pow(3);
        let mut data1 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data1);
        let mut data2 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data2);
        let mut data3 = vec![0_u8; size as usize];
        thread_rng().fill_bytes(&mut data3);

        let root_dir = tempdir().unwrap();
        let root_path = root_dir.path().display().to_string();
        println!("Repo root dir: {}", root_path);

        let backend = Box::new(FileSystem::new_test(&root_path));
        let mut repo = Repository::new(
            backend,
            Compression::ZStd { level: 1 },
            HMAC::SHA256,
            Encryption::new_aes256cbc(),
            &key,
        );

        println!("Adding Chunks");
        let key1 = repo.write_chunk(&data1).unwrap();
        let key2 = repo.write_chunk(&data2).unwrap();
        let key3 = repo.write_chunk(&data3).unwrap();

        println!("Reading Chunks");
        let out1 = repo.read_chunk(key1).unwrap();
        let out2 = repo.read_chunk(key2).unwrap();
        let out3 = repo.read_chunk(key3).unwrap();

        assert_eq!(data1, out1);
        assert_eq!(data2, out2);
        assert_eq!(data3, out3);
    }

}
