use crate::repository::backend::common;
use crate::repository::backend::*;
use crate::repository::EncryptedKey;
use anyhow::{anyhow, Result};
use async_std::task::block_on;
use futures::channel::oneshot;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::convert::TryInto;
use std::io::Cursor;
use std::sync::Arc;

type CursorSegment = Cursor<Vec<u8>>;

#[derive(Clone, Debug)]
pub struct Mem {
    data: Vec<common::SegmentHandle<CursorSegment>>,
    index: Arc<RwLock<HashMap<ChunkID, ChunkLocation>>>,
    manifest: Arc<RwLock<Vec<StoredArchive>>>,
    chunk_settings: Arc<RwLock<ChunkSettings>>,
    key: Arc<RwLock<Option<EncryptedKey>>>,
    count: Arc<Mutex<u64>>,
    len: u64,
}

impl Mem {
    pub fn new(chunk_settings: ChunkSettings) -> Mem {
        let max = usize::max_value().try_into().unwrap();
        let mut data = Vec::new();
        for _ in 0..num_cpus::get() {
            data.push(block_on(common::SegmentHandle::new(Cursor::new(Vec::new()), max)).unwrap());
        }
        Mem {
            data,
            index: Arc::new(RwLock::new(HashMap::new())),
            manifest: Arc::new(RwLock::new(Vec::new())),
            chunk_settings: Arc::new(RwLock::new(chunk_settings)),
            key: Arc::new(RwLock::new(None)),
            count: Arc::new(Mutex::new(0)),
            len: num_cpus::get() as u64,
        }
    }
}

impl Manifest for Mem {
    type Iterator = std::vec::IntoIter<StoredArchive>;
    fn last_modification(&self) -> DateTime<FixedOffset> {
        let manifest = self.manifest.read();
        let archive = &manifest[manifest.len() - 1];
        archive.timestamp()
    }
    fn chunk_settings(&self) -> ChunkSettings {
        *self.chunk_settings.read()
    }
    fn archive_iterator(&self) -> Self::Iterator {
        self.manifest.read().clone().into_iter()
    }
    fn write_chunk_settings(&mut self, settings: ChunkSettings) {
        let mut x = self.chunk_settings.write();
        *x = settings;
    }
    fn write_archive(&mut self, archive: StoredArchive) {
        let mut manifest = self.manifest.write();
        manifest.push(archive);
    }
    /// This implementation reconstructs the last modified time, so this does nothing
    #[cfg_attr(tarpaulin, skip)]
    fn touch(&mut self) {}
}

impl Index for Mem {
    fn lookup_chunk(&self, id: ChunkID) -> Option<ChunkLocation> {
        self.index.read().get(&id).copied()
    }
    fn set_chunk(&self, id: ChunkID, location: ChunkLocation) -> Result<()> {
        self.index.write().insert(id, location);
        Ok(())
    }
    /// This format is not persistant so this does nothing
    fn commit_index(&self) -> Result<()> {
        Ok(())
    }
    fn count_chunk(&self) -> usize {
        self.index.read().len()
    }
}

impl Backend for Mem {
    type Manifest = Self;
    type Segment = common::SegmentHandle<CursorSegment>;
    type Index = Self;
    /// Ignores the id
    fn get_segment(&self, id: u64) -> Result<Self::Segment> {
        Ok(self.data[id as usize].clone())
    }
    /// Returns a random number in [0,5)
    #[cfg_attr(tarpaulin, skip)]
    fn highest_segment(&self) -> u64 {
        let mut count = self.count.lock();
        let old = *count;
        *count += 1;
        old % self.len
    }
    /// Only has one segement, so this does nothing
    #[cfg_attr(tarpaulin, skip)]
    fn make_segment(&self) -> Result<u64> {
        Ok(self.highest_segment())
    }
    fn get_index(&self) -> Self::Index {
        self.clone()
    }
    fn write_key(&self, key: &EncryptedKey) -> Result<()> {
        let mut skey = self.key.write();
        *skey = Some(key.clone());
        Ok(())
    }
    fn read_key(&self) -> Result<EncryptedKey> {
        let key: &Option<EncryptedKey> = &self.key.read();
        if let Some(k) = key {
            Ok(k.clone())
        } else {
            Err(anyhow!("Tried to access an unset key"))
        }
    }
    fn get_manifest(&self) -> Self::Manifest {
        self.clone()
    }

    fn read_chunk(&self, locaton: SegmentDescriptor) -> oneshot::Receiver<Vec<u8>> {
        unimplemented!();
    }

    fn write_chunk(&self, chunk: Vec<u8>, id: ChunkID) -> oneshot::Receiver<SegmentDescriptor> {
        unimplemented!();
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
        backend.write_key(&encrypted_key);
        let output = backend.read_key().unwrap().decrypt(&key_key).unwrap();
        assert_eq!(key, output);
    }
}
