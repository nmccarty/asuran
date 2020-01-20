use crate::repository::backend::common;
use crate::repository::backend::*;
use crate::repository::EncryptedKey;
use anyhow::{anyhow, Result};
use async_std::sync::RwLock;
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
    len: u64,
}

impl Mem {
    pub fn new(chunk_settings: ChunkSettings) -> Mem {
        let max = usize::max_value().try_into().unwrap();
        let data = common::TaskedSegment::new(Cursor::new(Vec::new()), max, 0);
        Mem {
            data,
            index: Arc::new(RwLock::new(HashMap::new())),
            manifest: Arc::new(RwLock::new(Vec::new())),
            chunk_settings: Arc::new(RwLock::new(chunk_settings)),
            key: Arc::new(RwLock::new(None)),
            len: num_cpus::get() as u64,
        }
    }
}
#[async_trait]
impl Manifest for Mem {
    type Iterator = std::vec::IntoIter<StoredArchive>;
    async fn last_modification(&mut self) -> DateTime<FixedOffset> {
        let manifest = self.manifest.read().await;
        let archive = &manifest[manifest.len() - 1];
        archive.timestamp()
    }
    async fn chunk_settings(&mut self) -> ChunkSettings {
        *self.chunk_settings.read().await
    }
    async fn archive_iterator(&mut self) -> Self::Iterator {
        self.manifest.read().await.clone().into_iter()
    }
    async fn write_chunk_settings(&mut self, settings: ChunkSettings) {
        let mut x = self.chunk_settings.write().await;
        *x = settings;
    }
    async fn write_archive(&mut self, archive: StoredArchive) {
        let mut manifest = self.manifest.write().await;
        manifest.push(archive);
    }
    /// This implementation reconstructs the last modified time, so this does nothing
    #[cfg_attr(tarpaulin, skip)]
    async fn touch(&mut self) {}
}
#[async_trait]
impl Index for Mem {
    async fn lookup_chunk(&mut self, id: ChunkID) -> Option<SegmentDescriptor> {
        self.index.read().await.get(&id).copied()
    }
    async fn set_chunk(&mut self, id: ChunkID, location: SegmentDescriptor) -> Result<()> {
        self.index.write().await.insert(id, location);
        Ok(())
    }
    /// This format is not persistant so this does nothing
    async fn commit_index(&mut self) -> Result<()> {
        Ok(())
    }
    async fn count_chunk(&mut self) -> usize {
        self.index.read().await.len()
    }
}

#[async_trait]
impl Backend for Mem {
    type Manifest = Self;
    type Index = Self;
    fn get_index(&self) -> Self::Index {
        self.clone()
    }
    async fn write_key(&self, key: &EncryptedKey) -> Result<()> {
        let mut skey = self.key.write().await;
        *skey = Some(key.clone());
        Ok(())
    }
    async fn read_key(&self) -> Result<EncryptedKey> {
        let key: &Option<EncryptedKey> = &*self.key.read().await;
        if let Some(k) = key {
            Ok(k.clone())
        } else {
            Err(anyhow!("Tried to access an unset key"))
        }
    }
    fn get_manifest(&self) -> Self::Manifest {
        self.clone()
    }

    async fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Vec<u8>> {
        let mut data = self.data.clone();
        data.read_chunk(location).await
    }

    async fn write_chunk(&mut self, chunk: Vec<u8>, id: ChunkID) -> Result<SegmentDescriptor> {
        let mut data = self.data.clone();
        data.write_chunk(chunk, id).await
    }

    /// This backend does not persist, so a clean close is not required
    ///
    /// As such, we do nothing
    #[cfg_attr(tarpaulin, skip)]
    async fn close(self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::*;

    /// Makes sure accessing an unset key panics
    #[tokio::test]
    #[should_panic]
    async fn bad_key_access() {
        let backend = Mem::new(ChunkSettings::lightweight());
        backend.read_key().await.unwrap();
    }

    /// Checks to make sure setting and retriving a key works
    #[tokio::test]
    async fn key_sanity() {
        let backend = Mem::new(ChunkSettings::lightweight());
        let key = Key::random(32);
        let key_key = [0_u8; 128];
        let encrypted_key =
            EncryptedKey::encrypt(&key, 1024, 1, Encryption::new_aes256ctr(), &key_key);
        backend.write_key(&encrypted_key).await.unwrap();
        let output = backend.read_key().await.unwrap().decrypt(&key_key).unwrap();
        assert_eq!(key, output);
    }
}
