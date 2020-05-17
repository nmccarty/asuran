#![allow(unused_variables)]
use super::Result;
use crate::repository::backend::common::sync_backend::{
    BackendHandle, SyncBackend, SyncIndex, SyncManifest,
};
use crate::repository::backend::{
    Chunk, ChunkID, ChunkSettings, DateTime, EncryptedKey, FixedOffset, SegmentDescriptor,
    StoredArchive,
};
use crate::repository::Key;

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::path::Path;

pub use super::common::generic_flatfile::GenericFlatFile;

#[repr(transparent)]
#[derive(Debug)]
pub struct FlatFile(GenericFlatFile<File>);

impl FlatFile {
    /// Constructs a flatfile and wraps it
    ///
    /// See the documentation for `GenericFlatFile::new_raw` for further details
    pub fn new(
        repository_path: impl AsRef<Path>,
        settings: Option<ChunkSettings>,
        enc_key: Option<EncryptedKey>,
        key: Key,
        queue_depth: usize,
    ) -> Result<BackendHandle<FlatFile>> {
        let path = repository_path.as_ref().to_owned();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&path)?;
        let flat_file = GenericFlatFile::new_raw(file, path, settings, key, enc_key)?;
        Ok(BackendHandle::new(queue_depth, move || FlatFile(flat_file)))
    }

    /// Attempts to read the key from the flatfile repo at a given path
    pub fn load_encrypted_key(repository_path: impl AsRef<Path>) -> Result<EncryptedKey> {
        let path = repository_path.as_ref().to_owned();
        let file = OpenOptions::new().read(true).open(&path)?;
        GenericFlatFile::load_encrypted_key(file)
    }
}

impl SyncManifest for FlatFile {
    type Iterator = std::vec::IntoIter<StoredArchive>;
    fn last_modification(&mut self) -> Result<DateTime<FixedOffset>> {
        self.0.last_modification()
    }
    fn chunk_settings(&mut self) -> ChunkSettings {
        self.0.chunk_settings()
    }
    fn archive_iterator(&mut self) -> Self::Iterator {
        self.0.archive_iterator()
    }
    fn write_chunk_settings(&mut self, settings: ChunkSettings) -> Result<()> {
        self.0.write_chunk_settings(settings)
    }
    fn write_archive(&mut self, archive: StoredArchive) -> Result<()> {
        self.0.write_archive(archive)
    }
    fn touch(&mut self) -> Result<()> {
        self.0.touch()
    }
}

impl SyncIndex for FlatFile {
    fn lookup_chunk(&mut self, id: ChunkID) -> Option<SegmentDescriptor> {
        self.0.lookup_chunk(id)
    }
    fn set_chunk(&mut self, id: ChunkID, location: SegmentDescriptor) -> Result<()> {
        self.0.set_chunk(id, location)
    }
    fn known_chunks(&mut self) -> HashSet<ChunkID> {
        self.0.known_chunks()
    }
    fn commit_index(&mut self) -> Result<()> {
        self.0.commit_index()
    }
    fn chunk_count(&mut self) -> usize {
        self.0.chunk_count()
    }
}

impl SyncBackend for FlatFile {
    type SyncManifest = Self;
    type SyncIndex = Self;
    fn get_index(&mut self) -> &mut Self::SyncIndex {
        self
    }
    fn get_manifest(&mut self) -> &mut Self::SyncManifest {
        self
    }
    fn write_key(&mut self, key: EncryptedKey) -> Result<()> {
        self.0.write_key(key)
    }
    fn read_key(&mut self) -> Result<EncryptedKey> {
        self.0.read_key()
    }
    fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        self.0.read_chunk(location)
    }
    fn write_chunk(&mut self, chunk: Chunk) -> Result<SegmentDescriptor> {
        self.0.write_chunk(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::backend::Backend;
    use crate::repository::{Encryption, Key};
    use tempfile::tempdir;

    fn setup() -> (Key, EncryptedKey, ChunkSettings) {
        let key = Key::random(32);
        let pass = b"A Very strong password";
        let enc_key = EncryptedKey::encrypt(&key, 512, 1, Encryption::new_aes256ctr(), pass);
        (key, enc_key, ChunkSettings::lightweight())
    }

    // Create a new flatfile with a key and some settings, drop it, reload it, and check to see if
    // the key we read back is the same
    #[test]
    fn key_store_load() {
        smol::run(async {
            let (key, enc_key, settings) = setup();
            let directory = tempdir().unwrap();
            let file = directory.path().join("temp.asuran");
            // Generate the flatfile, close it, and drop it
            let mut flatfile =
                FlatFile::new(&file, Some(settings), Some(enc_key), key.clone(), 4).unwrap();
            flatfile.close().await;
            // Load it back up
            let flatfile = FlatFile::new(&file, None, None, key.clone(), 4).unwrap();
            // get the key
            let new_key = flatfile
                .read_key()
                .await
                .expect("Could not read key")
                .decrypt(b"A Very strong password")
                .expect("Could not decrypt key");

            assert_eq!(key, new_key);
        });
    }
}
