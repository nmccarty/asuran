#![allow(unused_variables)]
use super::Result;
use crate::repository::backend::common::sync_backend::{
    BackendHandle, SyncBackend, SyncIndex, SyncManifest,
};
use crate::repository::backend::{
    BackendError, Chunk, ChunkID, ChunkSettings, DateTime, EncryptedKey, FixedOffset,
    SegmentDescriptor, StoredArchive,
};

use asuran_core::repository::backend::flatfile::{Configuration, FlatFileTransaction, Header};

use rmp_serde as rmps;

use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const MAGIC_NUMBER: [u8; 8] = *b"ASURAN_F";

#[derive(Debug)]
pub struct FlatFile {
    /// A buffered reader over the repository file
    read: BufReader<File>,
    /// A buffered writer over the repository file
    write: BufWriter<File>,
    /// The in-memory index
    index: HashMap<ChunkID, SegmentDescriptor>,
    /// The in-memory manifest
    manifest: Vec<StoredArchive>,
    /// The header of the FlatFile, contains the chunk settings and key
    header: Header,
    /// The path of the repository file
    path: PathBuf,
    /// The offset of the first byte that is beyond the header
    past_header: u64,
}

impl FlatFile {
    /// Internal function for opening a flat file repository
    ///
    /// The backend this creates is not thread safe, see `FlatFile` for the thread safe
    /// implementation on top of this.
    ///
    /// Optionally sets the chunk settings.
    ///
    /// # Errors
    ///
    /// Will return Err if:
    /// 1. The file doesn't exist and creation failed
    /// 2. The file doesn't exist and chunk settings were not provided
    pub fn new_raw(
        repository_path: impl AsRef<Path>,
        settings: Option<ChunkSettings>,
        enc_key: Option<EncryptedKey>,
    ) -> Result<FlatFile> {
        let mut read;
        let mut write;
        let header;
        let past_header;
        // First check if the file exists, if it doesn't we will need to write a header
        if Path::exists(repository_path.as_ref()) {
            read = BufReader::new(File::open(repository_path.as_ref())?);
            write = BufWriter::new(
                OpenOptions::new()
                    .write(true)
                    .open(repository_path.as_ref())?,
            );
            header = Header::deserialize(&mut read).map_err(|_| {
                BackendError::ManifestError("Failed to load flatfile header".to_string())
            })?;
            past_header = read.seek(SeekFrom::Current(0))?;
        } else {
            // Check to see if we have chunk settings, and a key, if we don't we need to fail with
            // an error here
            if let Some(enc_key) = enc_key {
                if let Some(settings) = settings {
                    write = BufWriter::new(
                        OpenOptions::new()
                            .write(true)
                            .create(true)
                            .open(repository_path.as_ref())?,
                    );
                    read = BufReader::new(File::open(repository_path.as_ref())?);
                    // Create and write the header
                    header = Header {
                        magic_number: MAGIC_NUMBER,
                        implementation_uuid: *crate::IMPLEMENTATION_UUID.as_bytes(),
                        semver_major: crate::VERSION_PIECES[0],
                        semver_minor: crate::VERSION_PIECES[1],
                        semver_patch: crate::VERSION_PIECES[2],
                        configuration: Configuration {
                            key: enc_key,
                            chunk_settings: settings,
                        },
                    };
                    header.serialize(&mut write).map_err(|_| {
                        BackendError::ManifestError("Unable to write flatfile header".to_string())
                    })?;
                    past_header = write.seek(SeekFrom::Current(0))?;
                } else {
                    return Err(BackendError::Unknown(
                        "Attempted to create a new flatfile, but did not provide chunk settings"
                            .to_string(),
                    ));
                }
            } else {
                return Err(BackendError::Unknown(
                    "Attempted to create a new flatfile, but did not provide a key".to_string(),
                ));
            }
        }
        // Now that we have our file handles open, and the header taken care of, walk through
        // the file and construct the in memory index and manifest
        let mut index: HashMap<ChunkID, SegmentDescriptor> = HashMap::new();
        let mut manifest: Vec<StoredArchive> = Vec::new();
        // seek the reader to the first non-header byte
        let mut start = read.seek(SeekFrom::Start(past_header))?;
        while let Ok(tx) = rmps::decode::from_read(&mut read) {
            match tx {
                FlatFileTransaction::Insert { id, chunk } => {
                    let segment_descriptor = SegmentDescriptor {
                        segment_id: 0,
                        start,
                    };
                    index.insert(id, segment_descriptor);
                }
                FlatFileTransaction::Delete { id } => {
                    index.remove(&id);
                }
                FlatFileTransaction::ManifestInsert {
                    id,
                    name,
                    timestamp,
                } => {
                    manifest.push(StoredArchive {
                        id,
                        name,
                        timestamp,
                    });
                }
            }
            start = read.seek(SeekFrom::Current(0))?;
        }
        Ok(FlatFile {
            read,
            write,
            header,
            index,
            manifest,
            past_header,
            path: repository_path.as_ref().to_path_buf(),
        })
    }

    /// Constructs a flatfile and wraps it
    pub fn new(
        repository_path: impl AsRef<Path>,
        settings: Option<ChunkSettings>,
        enc_key: Option<EncryptedKey>,
    ) -> Result<BackendHandle<FlatFile>> {
        let flatfile = FlatFile::new_raw(repository_path, settings, enc_key)?;
        Ok(BackendHandle::new(flatfile))
    }

    /// Attempts to read the key from the flatfile repo at a given path
    pub fn load_encrypted_key(repository_path: impl AsRef<Path>) -> Result<EncryptedKey> {
        let mut read = File::open(repository_path)?;
        let header = Header::deserialize(&mut read)
            .map_err(|_| BackendError::Unknown("Invalid repository header".to_string()))?;
        Ok(header.configuration.key)
    }
}

impl SyncManifest for FlatFile {
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
        self.header.configuration.chunk_settings
    }
    fn archive_iterator(&mut self) -> Self::Iterator {
        self.manifest.clone().into_iter()
    }
    fn write_chunk_settings(&mut self, settings: ChunkSettings) -> Result<()> {
        Err(BackendError::ManifestError("Changing the default chunk settings with the flat file backend is currently unsupported".to_string()))
    }
    fn write_archive(&mut self, archive: StoredArchive) -> Result<()> {
        // First copy the archive into our in memory copy
        self.manifest.push(archive.clone());
        // Seek the file to the end
        self.write.seek(SeekFrom::End(0))?;
        // Now write it to the file
        rmps::encode::write(
            &mut self.write,
            &FlatFileTransaction::ManifestInsert {
                id: archive.id,
                name: archive.name,
                timestamp: archive.timestamp,
            },
        )?;
        Ok(())
    }
    fn touch(&mut self) -> Result<()> {
        // This backend does not support touching
        Ok(())
    }
}

impl SyncIndex for FlatFile {
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
        // There is no seperate index for this format
        Ok(())
    }
    fn chunk_count(&mut self) -> usize {
        self.index.len()
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
        Err(BackendError::Unknown(
            "Changing the key of a flat file repository is not supported at this time.".to_string(),
        ))
    }
    fn read_key(&mut self) -> Result<EncryptedKey> {
        Ok(self.header.configuration.key.clone())
    }
    fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        // Seek to the location of the chunk
        self.read.seek(SeekFrom::Start(location.start))?;
        // attempt to read the transaction
        let tx: FlatFileTransaction = rmps::decode::from_read(&mut self.read)?;
        if let FlatFileTransaction::Insert { chunk, .. } = tx {
            Ok(chunk)
        } else {
            Err(BackendError::SegmentError(
                "Invalid data pointer!".to_string(),
            ))
        }
    }
    fn write_chunk(&mut self, chunk: Chunk) -> Result<SegmentDescriptor> {
        let start = self.write.seek(SeekFrom::End(0))?;
        let id = chunk.get_id();
        let tx = FlatFileTransaction::Insert { chunk, id };
        rmps::encode::write(&mut self.write, &tx)?;
        Ok(SegmentDescriptor {
            segment_id: 0,
            start,
        })
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
    #[tokio::test(threaded_scheduler)]
    async fn key_store_load() {
        let (key, enc_key, settings) = setup();
        let directory = tempdir().unwrap();
        let file = directory.path().join("temp.asuran");
        // Generate the flatfile, close it, and drop it
        let mut flatfile = FlatFile::new(&file, Some(settings), Some(enc_key)).unwrap();
        flatfile.close().await;
        // Load it back up
        let flatfile = FlatFile::new(&file, None, None).unwrap();
        // get the key
        let new_key = flatfile
            .read_key()
            .await
            .expect("Could not read key")
            .decrypt(b"A Very strong password")
            .expect("Could not decrypt key");

        assert_eq!(key, new_key);
    }
}
