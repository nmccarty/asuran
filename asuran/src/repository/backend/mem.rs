use crate::repository::backend::common;
use crate::repository::backend::common::sync_backend::*;
use crate::repository::backend::*;
use crate::repository::{Chunk, EncryptedKey, Key};

use std::collections::HashMap;
use std::convert::TryInto;
use std::io::Cursor;
extern crate rmp_serde as rmps;

use super::Result;

#[derive(Debug)]
pub struct Mem {
    data: common::Segment<Cursor<Vec<u8>>>,
    index: HashMap<ChunkID, SegmentDescriptor>,
    manifest: Vec<StoredArchive>,
    chunk_settings: ChunkSettings,
    key: Option<EncryptedKey>,
    actual_key: Key,
    len: u64,
}

impl Mem {
    pub fn new_raw(chunk_settings: ChunkSettings, key: Key) -> Mem {
        let max = usize::max_value().try_into().expect("Running on a greater than 64 bit system. The mem backend is not supported in this configuration");
        // We are using fresh vectors for this instead of files, so this unwrap can not fail
        let data = common::Segment::new(
            Cursor::new(Vec::new()),
            Cursor::new(Vec::new()),
            max,
            chunk_settings,
            key.clone(),
        )
        .unwrap();
        Mem {
            data,
            index: HashMap::new(),
            manifest: Vec::new(),
            chunk_settings,
            actual_key: key,
            key: None,
            len: num_cpus::get() as u64,
        }
    }

    pub fn new(chunk_settings: ChunkSettings, key: Key) -> BackendHandle<Mem> {
        BackendHandle::new(Self::new_raw(chunk_settings, key))
    }
}

impl SyncManifest for Mem {
    type Iterator = std::vec::IntoIter<StoredArchive>;
    fn last_modification(&mut self) -> Result<DateTime<FixedOffset>> {
        if self.manifest.is_empty() {
            Err(BackendError::ManifestError(
                "No archives/timestamps present".to_string(),
            ))
        } else {
            let archive = &self.manifest[self.manifest.len() - 1];
            Ok(archive.timestamp())
        }
    }
    fn chunk_settings(&mut self) -> ChunkSettings {
        self.chunk_settings
    }
    fn archive_iterator(&mut self) -> Self::Iterator {
        self.manifest.clone().into_iter()
    }
    fn write_chunk_settings(&mut self, settings: ChunkSettings) -> Result<()> {
        self.chunk_settings = settings;
        Ok(())
    }
    fn write_archive(&mut self, archive: StoredArchive) -> Result<()> {
        self.manifest.push(archive);
        Ok(())
    }
    fn touch(&mut self) -> Result<()> {
        // This method doesnt really make sense on a non-persisting repository
        Ok(())
    }
}

impl SyncIndex for Mem {
    fn lookup_chunk(&mut self, id: ChunkID) -> Option<SegmentDescriptor> {
        self.index.get(&id).copied()
    }
    fn set_chunk(&mut self, id: ChunkID, location: SegmentDescriptor) -> Result<()> {
        self.index.insert(id, location);
        Ok(())
    }
    fn known_chunks(&mut self) -> HashSet<ChunkID> {
        self.index.keys().copied().collect::<HashSet<_>>()
    }
    fn commit_index(&mut self) -> Result<()> {
        // Does nothing, since this implementation does not commit
        Ok(())
    }
    fn chunk_count(&mut self) -> usize {
        self.index.len()
    }
}

impl SyncBackend for Mem {
    type SyncManifest = Self;
    type SyncIndex = Self;
    fn get_index(&mut self) -> &mut Self::SyncIndex {
        self
    }
    fn get_manifest(&mut self) -> &mut Self::SyncManifest {
        self
    }
    fn write_key(&mut self, key: EncryptedKey) -> Result<()> {
        self.key = Some(key);
        Ok(())
    }
    fn read_key(&mut self) -> Result<EncryptedKey> {
        if let Some(key) = self.key.clone() {
            Ok(key)
        } else {
            Err(BackendError::Unknown(
                "Tried to load an unset key".to_string(),
            ))
        }
    }
    fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        self.data.read_chunk(location.start)
    }
    fn write_chunk(&mut self, chunk: Chunk) -> Result<SegmentDescriptor> {
        let start = self.data.write_chunk(chunk)?;
        Ok(SegmentDescriptor {
            segment_id: 0,
            start,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::*;

    /// Makes sure accessing an unset key panics
    #[tokio::test(threaded_scheduler)]
    #[should_panic]
    async fn bad_key_access() {
        let key = Key::random(32);
        let backend = Mem::new(ChunkSettings::lightweight(), key);
        backend.read_key().await.unwrap();
    }

    /// Checks to make sure setting and retriving a key works
    #[tokio::test(threaded_scheduler)]
    async fn key_sanity() {
        let key = Key::random(32);
        let backend = Mem::new(ChunkSettings::lightweight(), key.clone());
        let key_key = [0_u8; 128];
        let encrypted_key =
            EncryptedKey::encrypt(&key, 1024, 1, Encryption::new_aes256ctr(), &key_key);
        backend.write_key(&encrypted_key).await.unwrap();
        let output = backend.read_key().await.unwrap().decrypt(&key_key).unwrap();
        assert_eq!(key, output);
    }
}
