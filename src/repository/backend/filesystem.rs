#![allow(clippy::filter_map)]

use crate::repository::backend::*;
use crate::repository::EncryptedKey;
use crate::repository::{Compression, Encryption, HMAC};
use anyhow::{Context, Result};
use async_trait::async_trait;
use parking_lot::RwLock;
use rmp_serde::encode::write;
use rmp_serde::{from_read, to_vec};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;

#[derive(Clone, Debug)]
pub struct FileSystem {
    root_directory: String,
    segments_per_folder: u64,
    segment_size: u64,
    manifest_file: Arc<RwLock<fs::File>>,
    index: Arc<RwLock<HashMap<ChunkID, ChunkLocation>>>,
    index_file: Arc<RwLock<fs::File>>,
}

impl FileSystem {
    /// Creates a new filesystem backend with the default number of segements per
    /// directory (250) and segment size (250MB)
    ///
    /// Will create an empty manifest with the chunk settings set to no compression, no encryption, and
    /// blake2b HMAC
    pub fn new(root_directory: &str) -> FileSystem {
        let segments_per_folder: u64 = 250;
        let segment_size: u64 = 250 * 10_u64.pow(6);
        // Create the directory if it doesn't exist
        fs::create_dir_all(root_directory).expect("Unable to create repository directory.");

        // Open the file handle for the manifest, creating it if it doesnt exist.
        let manifest_exists;
        let manifest_path = Path::new(root_directory).join("manifest");
        if manifest_path.exists() {
            manifest_exists = true;
        } else {
            fs::File::create(&manifest_path).expect("Unable to create manifest file.");
            manifest_exists = false;
        }
        let mut manifest_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&manifest_path)
            .expect("Failed to open manifest file. Check if you have permissions to the directory");

        if !manifest_exists {
            // Write an empty carrier to the manifest
            let empty_manifest = ManifestCarrier {
                timestamp: Local::now().with_timezone(Local::now().offset()),
                chunk_settings: ChunkSettings {
                    encryption: Encryption::NoEncryption,
                    compression: Compression::NoCompression,
                    hmac: HMAC::Blake2b,
                },
                archives: Vec::new(),
            };
            write(&mut manifest_file, &empty_manifest).expect("Unable to write manifest");
        }

        let manifest_file = Arc::new(RwLock::new(manifest_file));

        // Open the file handle for the index
        let index_exists;
        let index_path = Path::new(root_directory).join("index");
        if index_path.exists() {
            index_exists = true;
        } else {
            fs::File::create(&index_path).expect("Unable to create index file.");
            index_exists = false;
        }

        let mut index_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&index_path)
            .expect("Failed to open index file. Check if you have permissions to the directory");

        let index;
        if index_exists {
            // Read the index
            index = from_read(&index_file).expect("Unable to deserialize index");
        } else {
            // index is empty
            index = HashMap::new();
            // Write to file
            write(&mut index_file, &index).expect("Unable to initalize index");
        }

        FileSystem {
            root_directory: root_directory.to_string(),
            segments_per_folder,
            segment_size,
            manifest_file,
            index: Arc::new(RwLock::new(index)),
            index_file: Arc::new(RwLock::new(index_file)),
        }
    }

    pub fn new_test(root_directory: &str) -> FileSystem {
        let segments_per_folder: u64 = 2;
        let segment_size: u64 = 16 * 10_u64.pow(3);
        // Create the directory if it doesn't exist
        fs::create_dir_all(root_directory).expect("Unable to create repository directory.");

        // Open the file handle for the manifest, creating it if it doesnt exist.
        let manifest_path = Path::new(root_directory).join("manifest");
        if !manifest_path.exists() {
            fs::File::create(&manifest_path).expect("Unable to create manifest file");
        }
        let mut manifest_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&manifest_path)
            .expect("Failed to open manifest file. Check if you have permissions to the directory");

        // Write an empty carrier to the manifest
        let empty_manifest = ManifestCarrier {
            timestamp: Local::now().with_timezone(Local::now().offset()),
            chunk_settings: ChunkSettings {
                encryption: Encryption::NoEncryption,
                compression: Compression::NoCompression,
                hmac: HMAC::Blake2b,
            },
            archives: Vec::new(),
        };
        write(&mut manifest_file, &empty_manifest).expect("Unable to write manifest");

        let manifest_file = Arc::new(RwLock::new(manifest_file));

        // Open the file handle for the index
        let index_exists;
        let index_path = Path::new(root_directory).join("index");
        if index_path.exists() {
            index_exists = true;
        } else {
            fs::File::create(&index_path).expect("Unable to create index file.");
            index_exists = false;
        }

        let mut index_file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&index_path)
            .expect("Failed to open index file. Check if you have permissions to the directory");

        let index;
        if index_exists {
            // Read the index
            index = from_read(&index_file).expect("Unable to deserialize index");
        } else {
            // index is empty
            index = HashMap::new();
            // Write to file
            write(&mut index_file, &index).expect("Unable to initalize index");
        }

        FileSystem {
            root_directory: root_directory.to_string(),
            segments_per_folder,
            segment_size,
            manifest_file,
            index: Arc::new(RwLock::new(index)),
            index_file: Arc::new(RwLock::new(index_file)),
        }
    }
}

impl Backend for FileSystem {
    type Manifest = Self;
    type Segment = FileSystemSegment;
    type Index = Self;
    fn get_segment(&self, id: u64) -> Result<FileSystemSegment> {
        let dir_name = (id / self.segments_per_folder).to_string();
        let path = Path::new(&self.root_directory)
            .join(Path::new(&dir_name))
            .join(Path::new(&id.to_string()));
        let file = fs::OpenOptions::new().read(true).write(true).open(path)?;
        let segment = FileSystemSegment {
            file,
            max_size: self.segment_size,
        };
        Ok(segment)
    }

    fn highest_segment(&self) -> u64 {
        WalkDir::new(self.root_directory.clone())
            .into_iter()
            .filter_map(std::result::Result::ok)
            .map(|i| {
                let str = i.path().file_name().unwrap().to_str();
                str.unwrap().to_string()
            })
            .filter_map(|i| i.parse::<u64>().ok())
            .fold(0, std::cmp::max)
    }

    fn make_segment(&self) -> Result<u64> {
        let id = self.highest_segment() + 1;
        let dir_name = (id / self.segments_per_folder).to_string();
        let dir_path = Path::new(&self.root_directory).join(Path::new(&dir_name));
        // Create directory if it doesnt exist
        fs::create_dir_all(dir_path.clone())?;
        // Create file
        let path = dir_path.join(Path::new(&id.to_string()));
        fs::File::create(path.clone())
            .with_context(|| format!("Failed to create new segment file at {:?}", path))?;
        Ok(id)
    }

    fn get_index(&self) -> Self::Index {
        self.clone()
    }

    fn write_key(&self, key: &EncryptedKey) -> Result<()> {
        let path = Path::new(&self.root_directory).join(Path::new("keyfile"));
        let mut file = fs::File::create(path)?;
        let bytes = to_vec(key).unwrap();
        file.write_all(&bytes)?;
        Ok(())
    }

    fn read_key(&self) -> Result<EncryptedKey> {
        let path = Path::new(&self.root_directory).join(Path::new("keyfile"));
        let file = fs::File::open(path.clone())
            .with_context(|| format!("Failed to read key file from {:?}", path))?;
        from_read(&file).context("Failed to deserialize key")
    }

    fn get_manifest(&self) -> Self::Manifest {
        self.clone()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ManifestCarrier {
    timestamp: DateTime<FixedOffset>,
    chunk_settings: ChunkSettings,
    archives: Vec<StoredArchive>,
}

impl Manifest for FileSystem {
    type Iterator = std::vec::IntoIter<StoredArchive>;
    fn last_modification(&self) -> DateTime<FixedOffset> {
        let mut file_guard = self.manifest_file.write();
        let file: &mut fs::File = &mut file_guard;
        file.seek(SeekFrom::Start(0)).unwrap();
        let carrier: ManifestCarrier = from_read(file).unwrap();
        carrier.timestamp
    }

    fn chunk_settings(&self) -> ChunkSettings {
        let mut file_guard = self.manifest_file.write();
        let file: &mut fs::File = &mut file_guard;
        file.seek(SeekFrom::Start(0)).unwrap();
        let carrier: ManifestCarrier = from_read(file).unwrap();
        carrier.chunk_settings
    }

    fn archive_iterator(&self) -> std::vec::IntoIter<StoredArchive> {
        let mut file_guard = self.manifest_file.write();
        let file: &mut fs::File = &mut file_guard;
        file.seek(SeekFrom::Start(0)).unwrap();
        let carrier: ManifestCarrier = from_read(file).unwrap();
        let mut archives = carrier.archives;
        archives.reverse();

        archives.into_iter()
    }

    fn write_chunk_settings(&mut self, settings: ChunkSettings) {
        let mut file_guard = self.manifest_file.write();
        let file: &mut fs::File = &mut file_guard;
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut carrier: ManifestCarrier = from_read(file).unwrap();
        // Update settings
        carrier.chunk_settings = settings;
        // Update time
        carrier.timestamp = Local::now().with_timezone(Local::now().offset());
        let file: &mut fs::File = &mut file_guard;
        file.seek(SeekFrom::Start(0)).unwrap();
        // Empty file and overwrite
        file.set_len(0)
            .expect("Unable to empty file writing settings.");
        write(file, &carrier).expect("Unable to write settings.");
    }

    fn write_archive(&mut self, archive: StoredArchive) {
        let mut file_guard = self.manifest_file.write();
        let file: &mut fs::File = &mut file_guard;
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut carrier: ManifestCarrier = from_read(file).unwrap();
        // Update settings
        carrier.archives.push(archive);
        // Update time
        carrier.timestamp = Local::now().with_timezone(Local::now().offset());
        let file: &mut fs::File = &mut file_guard;
        file.seek(SeekFrom::Start(0)).unwrap();
        // Empty file and overwrite
        file.set_len(0)
            .expect("Unable to empty file writing settings.");
        write(file, &carrier).expect("Unable to write settings.");
    }

    fn touch(&mut self) {
        let mut file_guard = self.manifest_file.write();
        let file: &mut fs::File = &mut file_guard;
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut carrier: ManifestCarrier = from_read(file).unwrap();
        // Update time
        carrier.timestamp = Local::now().with_timezone(Local::now().offset());
        let file: &mut fs::File = &mut file_guard;
        file.seek(SeekFrom::Start(0)).unwrap();
        // Empty file and overwrite
        file.set_len(0)
            .expect("Unable to empty file writing settings.");
        write(file, &carrier).expect("Unable to write settings.");
    }
}

impl Index for FileSystem {
    fn lookup_chunk(&self, id: ChunkID) -> Option<ChunkLocation> {
        self.index.read().get(&id).copied()
    }
    fn set_chunk(&self, id: ChunkID, location: ChunkLocation) -> Result<()> {
        let mut index = self.index.write();
        index.insert(id, location);
        Ok(())
    }
    fn commit_index(&self) -> Result<()> {
        let index_guard = self.index.read();
        let index: &HashMap<ChunkID, ChunkLocation> = &index_guard;
        let mut file_guard = self.index_file.write();
        let file: &mut fs::File = &mut file_guard;
        // go to start of file and empty it
        file.seek(SeekFrom::Start(0))?;
        file.set_len(0)?;
        // Write index
        write(file, &index)?;

        Ok(())
    }
    fn count_chunk(&self) -> usize {
        self.index.read().len()
    }
}

pub struct FileSystemSegment {
    file: fs::File,
    max_size: u64,
}

#[async_trait]
impl Segment for FileSystemSegment {
    fn free_bytes(&mut self) -> u64 {
        let file_size = self.file.metadata().unwrap().len();
        if file_size > self.max_size {
            0
        } else {
            self.max_size - file_size
        }
    }

    async fn read_chunk(&mut self, start: u64, length: u64) -> Result<Vec<u8>> {
        let mut output = vec![0_u8; length as usize];
        self.file.seek(SeekFrom::Start(start))?;
        self.file.read_exact(&mut output)?;
        Ok(output)
    }

    async fn write_chunk(&mut self, chunk: &[u8], _id: ChunkID) -> Result<(u64, u64)> {
        let length = chunk.len() as u64;
        let location = self.file.seek(SeekFrom::End(1))?;
        self.file.write_all(chunk)?;

        Ok((location, length))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{Encryption, Key};
    use tempfile::tempdir;

    #[test]
    fn key_store_restore() {
        let test_dir = tempdir().unwrap();
        let backend = FileSystem::new(&test_dir.path().display().to_string());
        let encryption = Encryption::new_aes256ctr();

        let input_key = Key::random(32);
        let user_key = "A sercure password".as_bytes();
        let enc_input_key = EncryptedKey::encrypt_defaults(&input_key, encryption, user_key);

        backend.write_key(&enc_input_key).unwrap();

        let enc_output_key = backend.read_key().unwrap();
        let output_key = enc_output_key.decrypt(user_key).unwrap();

        assert_eq!(input_key, output_key);
    }

    #[test]
    fn store_restore_archive() {
        let test_dir = tempdir().unwrap();
        let mut backend = FileSystem::new(&test_dir.path().display().to_string());

        let proto_archive = StoredArchive::dummy_archive();

        // Write the archive
        backend.write_archive(proto_archive.clone());

        // Read it back
        let archive = backend.archive_iterator().next().unwrap();

        assert_eq!(proto_archive, archive);
    }

    #[test]
    fn touch_updates_time() {
        let test_dir = tempdir().unwrap();
        let mut backend = FileSystem::new(&test_dir.path().display().to_string());

        let timestamp1 = backend.last_modification();
        backend.touch();
        let timestamp2 = backend.last_modification();

        assert!(timestamp2 > timestamp1);
    }

    #[test]
    fn insertion_order() {
        let test_dir = tempdir().unwrap();
        let mut backend = FileSystem::new(&test_dir.path().display().to_string());

        let dummy_archive_1 = StoredArchive::dummy_archive();
        backend.write_archive(dummy_archive_1.clone());

        let dummy_archive_2 = StoredArchive::dummy_archive();
        backend.write_archive(dummy_archive_2.clone());

        let mut iter = backend.archive_iterator();
        let restore_2 = iter.next().unwrap();
        let restore_1 = iter.next().unwrap();

        assert_eq!(restore_1, dummy_archive_1);
        assert_eq!(restore_2, dummy_archive_2);
        assert_ne!(restore_1, restore_2);
    }

    #[test]
    fn load_unload_load() {
        let test_dir = tempdir().unwrap();
        let dummy_archive_1 = StoredArchive::dummy_archive();
        let dummy_archive_2 = StoredArchive::dummy_archive();
        {
            let mut backend = FileSystem::new(&test_dir.path().display().to_string());
            backend.write_archive(dummy_archive_1.clone());
        }
        {
            let mut backend = FileSystem::new(&test_dir.path().display().to_string());
            backend.write_archive(dummy_archive_2.clone());
        }
        {
            let mut backend = FileSystem::new(&test_dir.path().display().to_string());
            let mut iter = backend.archive_iterator();
            let restore_2 = iter.next().unwrap();
            let restore_1 = iter.next().unwrap();

            assert_eq!(restore_1, dummy_archive_1);
            assert_eq!(restore_2, dummy_archive_2);
            assert_ne!(restore_1, restore_2);
        }
    }
}
