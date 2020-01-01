use crate::repository::backend::common;
use crate::repository::backend::*;
use crate::repository::EncryptedKey;
use anyhow::{anyhow, Result};
use futures::channel::oneshot;
use futures::executor::ThreadPool;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::convert::TryInto;
use std::io::Cursor;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Mem {
    data: common::TaskedSegment<Cursor<Vec<u8>>>,
    index: Arc<RwLock<HashMap<ChunkID, SegmentDescriptor>>>,
    manifest: Arc<RwLock<Vec<StoredArchive>>>,
    chunk_settings: Arc<RwLock<ChunkSettings>>,
    key: Arc<RwLock<Option<EncryptedKey>>>,
    count: Arc<Mutex<u64>>,
    len: u64,
}

impl Mem {
    pub fn new(chunk_settings: ChunkSettings, pool: &ThreadPool) -> Mem {
        let max = usize::max_value().try_into().unwrap();
        let data = common::TaskedSegment::new(Cursor::new(Vec::new()), max, 0, pool);
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
    fn lookup_chunk(&self, id: ChunkID) -> Option<SegmentDescriptor> {
        self.index.read().get(&id).copied()
    }
    fn set_chunk(&self, id: ChunkID, location: SegmentDescriptor) -> Result<()> {
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

#[async_trait]
impl Backend for Mem {
    type Manifest = Self;
    type Index = Self;
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

    async fn read_chunk(&self, location: SegmentDescriptor) -> oneshot::Receiver<Result<Vec<u8>>> {
        let mut data = self.data.clone();
        data.read_chunk(location).await
    }

    async fn write_chunk(
        &self,
        chunk: Vec<u8>,
        id: ChunkID,
    ) -> oneshot::Receiver<Result<SegmentDescriptor>> {
        let mut data = self.data.clone();
        data.write_chunk(chunk, id).await
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
        let pool = ThreadPool::new().unwrap();
        let backend = Mem::new(ChunkSettings::lightweight(), &pool);
        backend.read_key().unwrap();
    }

    /// Checks to make sure setting and retriving a key works
    #[test]
    fn key_sanity() {
        let pool = ThreadPool::new().unwrap();
        let backend = Mem::new(ChunkSettings::lightweight(), &pool);
        let key = Key::random(32);
        let key_key = [0_u8; 128];
        let encrypted_key =
            EncryptedKey::encrypt(&key, 1024, 1, Encryption::new_aes256ctr(), &key_key);
        backend.write_key(&encrypted_key).unwrap();
        let output = backend.read_key().unwrap().decrypt(&key_key).unwrap();
        assert_eq!(key, output);
    }
}
