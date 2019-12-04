use crate::repository::backend::*;
use crate::repository::EncryptedKey;
use crate::repository::backend::common as common;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::convert::TryInto;
use std::sync::{Arc, RwLock};
use std::io::Cursor;

type CursorSegment =  common::Segment<Cursor<Vec<u8>>>;

#[derive(Clone, Debug)]
pub struct Mem {
    data: Arc<RwLock<CursorSegment>>,
    index: Arc<RwLock<HashMap<ChunkID, ChunkLocation>>>,
    manifest: Arc<RwLock<Vec<StoredArchive>>>,
    chunk_settings: Arc<RwLock<ChunkSettings>>,
    key: Arc<RwLock<Option<EncryptedKey>>>,
}

impl Mem {
    pub fn new(chunk_settings: ChunkSettings) -> Mem {
        let max = usize::max_value().try_into().unwrap();
        let segment = common::Segment::new(Cursor::new(Vec::new()),max);
        Mem {
            data: Arc::new(RwLock::new(segment)),
            index: Arc::new(RwLock::new(HashMap::new())),
            manifest: Arc::new(RwLock::new(Vec::new())),
            chunk_settings: Arc::new(RwLock::new(chunk_settings)),
            key: Arc::new(RwLock::new(None)),
        }
    }
}

impl Manifest for Mem {
    type Iterator = std::vec::IntoIter<StoredArchive>;
    fn last_modification(&self) -> DateTime<FixedOffset> {
        let manifest = self.manifest.read().unwrap();
        let archive = &manifest[manifest.len() - 1];
        archive.timestamp()
    }
    fn chunk_settings(&self) -> ChunkSettings {
        *self.chunk_settings.read().unwrap()
    }
    fn archive_iterator(&self) -> Self::Iterator {
        self.manifest.read().unwrap().clone().into_iter()
    }
    fn write_chunk_settings(&mut self, settings: ChunkSettings) {
        let mut x = self.chunk_settings.write().unwrap();
        *x = settings;
    }
    fn write_archive(&mut self, archive: StoredArchive) {
        let mut manifest = self.manifest.write().unwrap();
        manifest.push(archive);
    }
    /// This implementation reconstructs the last modified time, so this does nothing
    #[cfg_attr(tarpaulin, skip)]
    fn touch(&mut self) {}
}

impl Segment for Mem {
    /// Always returns u64::max
    #[cfg_attr(tarpaulin, skip)]
    fn free_bytes(&mut self) -> u64 {
        let mut segment = self.data.write().unwrap();
        segment.free_bytes()
    }

    /// Ignores the length
    fn read_chunk(&mut self, start: u64, length: u64) -> Result<Vec<u8>> {
        let mut segment = self.data.write().unwrap();
        segment.read_chunk(start, length)
    }

    /// Ignores the length
    fn write_chunk(&mut self, chunk: &[u8], id: ChunkID) -> Result<(u64, u64)> {
        let mut segment = self.data.write().unwrap();
        segment.write_chunk(chunk, id)
    }
}

impl Index for Mem {
    fn lookup_chunk(&self, id: ChunkID) -> Option<ChunkLocation> {
        self.index.read().unwrap().get(&id).copied()
    }
    fn set_chunk(&self, id: ChunkID, location: ChunkLocation) -> Result<()> {
        self.index.write().unwrap().insert(id, location);
        Ok(())
    }
    /// This format is not persistant so this does nothing
    fn commit_index(&self) -> Result<()> {
        Ok(())
    }
    fn count_chunk(&self) -> usize {
        self.index.read().unwrap().len()
    }
}

impl Backend for Mem {
    type Manifest = Self;
    type Segment = Self;
    type Index = Self;
    /// Ignores the id
    fn get_segment(&self, _id: u64) -> Result<Self::Segment> {
        Ok(self.clone())
    }
    /// Always returns 0
    #[cfg_attr(tarpaulin, skip)]
    fn highest_segment(&self) -> u64 {
        0
    }
    /// Only has one segement, so this does nothing
    #[cfg_attr(tarpaulin, skip)]
    fn make_segment(&self) -> Result<u64> {
        Ok(0)
    }
    fn get_index(&self) -> Self::Index {
        self.clone()
    }
    fn write_key(&self, key: &EncryptedKey) -> Result<()> {
        let mut skey = self.key.write().unwrap();
        *skey = Some(key.clone());
        Ok(())
    }
    fn read_key(&self) -> Result<EncryptedKey> {
        let key = self.key.read().unwrap();
        if key.is_some() {
            Ok(key.as_ref().unwrap().clone())
        } else {
            Err(anyhow!("Attempted to read a key that has not been set"))
        }
    }
    fn get_manifest(&self) -> Self::Manifest {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::*;

    /// Makes sure accessing an unset key panics
    #[test]
    #[should_panic]
    fn bad_key_access() {
        let backend = Mem::new(ChunkSettings::lightweight());
        backend.read_key().unwrap();
    }

    /// Checks to make sure setting and retriving a key works
    #[test]
    fn key_sanity() {
        let backend = Mem::new(ChunkSettings::lightweight());
        let key = Key::random(32);
        let key_key = [0_u8; 128];
        let encrypted_key =
            EncryptedKey::encrypt(&key, 1024, 1, Encryption::new_aes256ctr(), &key_key);
        backend.write_key(&encrypted_key).unwrap();
        let output = backend.read_key().unwrap().decrypt(&key_key).unwrap();
        assert_eq!(key, output);
    }
}
