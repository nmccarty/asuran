use crate::chunker::AsyncChunker;
use crate::repository::backend::common::manifest::ManifestTransaction;
use crate::repository::{BackendClone, ChunkID, Repository};

pub use asuran_core::manifest::archive::{Archive, ChunkLocation, Extent};
pub use asuran_core::manifest::listing::{Listing, Node, NodeType};

use chrono::prelude::*;
use futures::future::join_all;
use futures::stream::StreamExt;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tokio::task;

use std::collections::HashMap;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::Arc;

/// Error for all the things that can go wrong with handling Archives
#[derive(Error, Debug)]
pub enum ArchiveError {
    #[error("Chunker Error")]
    Chunker(#[from] crate::chunker::ChunkerError),
    #[error("I/O Error")]
    IO(#[from] std::io::Error),
    #[error("Async Task Join Error")]
    AsyncJoin(#[from] task::JoinError),
    #[error("")]
    Repository(#[from] crate::repository::RepositoryError),
}

type Result<T> = std::result::Result<T, ArchiveError>;

/// A 'heavy' pointer to a an `Archive` in a repository.
///
/// Contains the `ChunkID` of the chunk the `Archive` is serialized in, as well as
/// its date of creation.
///
/// Currently also contains the name of the `Archive`, but adding this was a mistake
/// as it leaks information that should not be leaked, so it will be removed soon.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct StoredArchive {
    /// The name of the archive
    pub name: String,
    /// Pointer the the archive metadata in the repository
    pub id: ChunkID,
    /// Time the archive was started it
    ///
    /// Used to prevent replay attackts
    pub timestamp: DateTime<FixedOffset>,
}

impl StoredArchive {
    /// Loads the archive metadata from the repository and unpacks it for use
    pub async fn load(&self, repo: &mut Repository<impl BackendClone>) -> Result<ActiveArchive> {
        let bytes = repo.read_chunk(self.id).await?;
        let mut de = Deserializer::new(&bytes[..]);
        let dumb_archive: Archive =
            Deserialize::deserialize(&mut de).expect("Unable to deserialize archive");
        let archive = ActiveArchive::from_archive(dumb_archive);
        Ok(archive)
    }

    /// Constructs a dummy archive object used for testing
    #[cfg(test)]
    pub fn dummy_archive() -> StoredArchive {
        StoredArchive {
            name: "Test".to_string(),
            id: ChunkID::manifest_id(),
            timestamp: Local::now().with_timezone(Local::now().offset()),
        }
    }

    /// Returns the timestamp of the archive
    pub fn timestamp(&self) -> DateTime<FixedOffset> {
        self.timestamp
    }

    /// Returns the pointer to the archive
    pub fn id(&self) -> ChunkID {
        self.id
    }

    /// Returns the name of the archive
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl From<ManifestTransaction> for StoredArchive {
    fn from(item: ManifestTransaction) -> Self {
        StoredArchive {
            name: item.name().to_string(),
            id: item.pointer(),
            timestamp: item.timestamp(),
        }
    }
}

#[derive(Clone, Debug)]
/// A currently open and able to be modified `Archive`
///
/// This is basically the same thing as an `Archive`, but has async/await aware
/// synchronization types wrapping some shared state, allowing the archive to be
/// used in multiple tasks at once.
pub struct ActiveArchive {
    /// The name of this archive
    ///
    /// Can be used to pull this archive from the manifest later.
    ///
    /// Can be any arbitray string
    name: String,
    /// Locations of all the chunks of the objects in this archive
    objects: Arc<RwLock<HashMap<String, Vec<ChunkLocation>>>>,
    /// The namespace this archive puts and gets objects in
    ///
    /// A namespace is a colon seperated lists of strings.
    ///
    /// The default namespace is :
    ///
    /// Namespaces are stored here as a vector of their parts
    namespace: Vec<String>,
    /// Time stamp is set at archive creation, this is different than the one
    /// set in stored archive
    timestamp: DateTime<FixedOffset>,
    /// The object listing of the archive
    listing: Arc<RwLock<Listing>>,
}

impl ActiveArchive {
    /// Creates a new, empty `ActiveArchive`
    pub fn new(name: &str) -> Self {
        ActiveArchive {
            name: name.to_string(),
            objects: Arc::new(RwLock::new(HashMap::new())),
            namespace: Vec::new(),
            timestamp: Local::now().with_timezone(Local::now().offset()),
            listing: Arc::new(RwLock::new(Listing::default())),
        }
    }

    /// Places an object into a archive, as a whole, without regard to sparsity
    ///
    /// Will read holes as 0s
    ///
    /// This is implemented as a thin wrapper around `put_sparse_object`
    pub async fn put_object<R: Read + Send + 'static>(
        &mut self,
        chunker: &impl AsyncChunker,
        repository: &mut Repository<impl BackendClone>,
        path: &str,
        from_reader: R,
    ) -> Result<()> {
        // We take advantage of put_sparse_object's behavior of reading past the given end if the
        // given reader is actually longer
        let extent = Extent { start: 0, end: 0 };
        let readers = vec![(extent, from_reader)];
        self.put_sparse_object(chunker, repository, path, readers)
            .await
    }

    /// Inserts a sparse object into the archive
    ///
    /// Requires that the object be pre-split into extents
    pub async fn put_sparse_object<R: Read + Send + 'static>(
        &mut self,
        chunker: &impl AsyncChunker,
        repository: &mut Repository<impl BackendClone>,
        path: &str,
        from_readers: Vec<(Extent, R)>,
    ) -> Result<()> {
        let mut locations: Vec<ChunkLocation> = Vec::new();
        let path = self.canonical_namespace() + path.trim();

        for (extent, read) in from_readers {
            let max_futs = 100;
            let mut futs = VecDeque::new();
            let mut slices = chunker.async_chunk(read);
            let mut start = extent.start;
            while let Some(result) = slices.next().await {
                let data = result?;
                let end = start + (data.len() as u64);

                let mut repository = repository.clone();
                futs.push_back(task::spawn(async move {
                    let id = repository.write_chunk(data).await?.0;
                    let result: Result<ChunkLocation> = Ok(ChunkLocation {
                        id,
                        start,
                        length: end - start + 1,
                    });
                    result
                }));
                while futs.len() >= max_futs {
                    // This unwrap is sound, since we can only be here if futs has elements in it
                    let loc = futs.pop_front().unwrap().await??;
                    locations.push(loc);
                }
                start = end + 1;
            }
            let locs = join_all(futs).await;
            for loc in locs {
                let loc = loc?;
                locations.push(loc?);
            }
        }

        let mut objects = self.objects.write().await;
        objects.insert(path.to_string(), locations);

        Ok(())
    }

    /// Inserts an object into the archive without writing any bytes
    pub async fn put_empty(&mut self, path: &str) {
        let locations: Vec<ChunkLocation> = Vec::new();
        let mut objects = self.objects.write().await;
        objects.insert(path.to_string(), locations);
    }

    /// Retreives an object from the archive, without regard to sparsity.
    ///
    /// Will fill in holes with zeros.
    pub async fn get_object(
        &self,
        repository: &mut Repository<impl BackendClone>,
        path: &str,
        mut restore_to: impl Write,
    ) -> Result<()> {
        let path = self.canonical_namespace() + path.trim();
        // Get chunk locations
        let objects = self.objects.read().await;
        let locations = objects.get(&path.to_string()).cloned();
        let mut locations = if let Some(locations) = locations {
            locations
        } else {
            return Ok(());
        };
        locations.sort_unstable();
        let mut last_index = locations[0].start;
        for location in &locations {
            let id = location.id;
            // If a chunk is not included, fill the space inbween it and the last with zeros
            let start = location.start;
            if start > last_index + 1 {
                let zero = [0_u8];
                for _ in last_index + 1..start {
                    restore_to.write_all(&zero)?;
                }
            }
            let bytes = repository.read_chunk(id).await?;

            restore_to.write_all(&bytes)?;
            last_index = start + location.length - 1;
        }

        Ok(())
    }

    /// Retrieve a single extent of an object from the repository
    ///
    /// Will write past the end of the last chunk ends after the extent
    pub async fn get_extent(
        &self,
        repository: &mut Repository<impl BackendClone>,
        path: &str,
        extent: Extent,
        mut restore_to: impl Write,
    ) -> Result<()> {
        let path = self.canonical_namespace() + path.trim();
        let objects = self.objects.read().await;

        let locations = objects.get(&path.to_string()).cloned();
        let mut locations = if let Some(locations) = locations {
            locations
        } else {
            return Ok(());
        };
        locations.sort_unstable();
        let locations = locations
            .iter()
            .filter(|x| x.start >= extent.start && x.start <= extent.end);
        // If there are any holes in the extent, fill them in with zeros
        let mut last_index = extent.start;
        for location in locations {
            let id = location.id;
            // Perform filling if needed
            let start = location.start;
            if start > last_index + 1 {
                let zero = [0_u8];
                for _ in last_index + 1..start {
                    restore_to.write_all(&zero)?;
                }
            }
            let bytes = repository.read_chunk(id).await?;
            restore_to.write_all(&bytes)?;
            last_index = start + location.length - 1;
        }

        Ok(())
    }

    /// Retrieves a sparse object from the repository
    ///
    /// Will skip over holes
    ///
    /// Will not write to extents that are not specified
    pub async fn get_sparse_object(
        &self,
        repository: &mut Repository<impl BackendClone>,
        path: &str,
        mut to_writers: Vec<(Extent, impl Write)>,
    ) -> Result<()> {
        for (extent, restore_to) in &mut to_writers {
            self.get_extent(repository, path, *extent, restore_to)
                .await?;
        }
        Ok(())
    }

    /// Returns the namespace of this archive in string form
    pub fn canonical_namespace(&self) -> String {
        self.namespace.join(":") + ":"
    }

    /// Changes namespace by adding the name to the end of the namespace
    ///
    /// Returns a new archive
    pub fn namespace_append(&self, name: &str) -> ActiveArchive {
        let mut new_namespace = self.namespace.clone();
        new_namespace.push(name.to_string());
        let mut archive = self.clone();
        archive.namespace = new_namespace;
        archive
    }

    /// Stores archive metatdat in the repository, producing a Stored Archive
    ///  object, and consuming the Archive in the process.
    ///
    /// Returns the key of the serialized archive in the repository
    pub async fn store(self, repo: &mut Repository<impl BackendClone>) -> StoredArchive {
        let dumb_archive = self.into_archive().await;
        let mut bytes = Vec::<u8>::new();
        dumb_archive
            .serialize(&mut Serializer::new(&mut bytes))
            .expect("Unable to serialize archive.");

        let id = repo
            .write_chunk(bytes)
            .await
            .expect("Unable to write archive metatdata to repository.")
            .0;

        repo.commit_index().await;

        StoredArchive {
            id,
            name: dumb_archive.name,
            timestamp: dumb_archive.timestamp,
        }
    }

    #[cfg_attr(tarpaulin, skip)]
    /// Provides the name of the archive
    pub fn name(&self) -> &str {
        &self.name
    }

    #[cfg_attr(tarpaulin, skip)]
    /// Provides the timestamp of the archive
    pub fn timestamp(&self) -> &DateTime<FixedOffset> {
        &self.timestamp
    }

    /// Converts an Archive into an `ActiveArchive`
    pub fn from_archive(archive: Archive) -> ActiveArchive {
        ActiveArchive {
            name: archive.name,
            objects: Arc::new(RwLock::new(archive.objects)),
            namespace: archive.namespace,
            timestamp: archive.timestamp,
            listing: Arc::new(RwLock::new(archive.listing)),
        }
    }

    /// Converts self into an Archive
    pub async fn into_archive(self) -> Archive {
        Archive {
            name: self.name,
            objects: self.objects.read().await.clone(),
            namespace: self.namespace,
            timestamp: self.timestamp,
            listing: self.listing.read().await.clone(),
        }
    }

    /// Gets a copy of the listing from the archive
    pub async fn listing(&self) -> Listing {
        self.listing.read().await.clone()
    }

    /// Replaces the listing with the provided value
    pub async fn set_listing(&self, listing: Listing) {
        *self.listing.write().await = listing;
    }
}

#[cfg(test)]
#[cfg_attr(tarpaulin, skip)]
mod tests {
    use super::*;
    use crate::chunker::*;
    use crate::repository::backend::mem::Mem;
    use crate::repository::ChunkSettings;
    use crate::repository::Key;
    use rand::prelude::*;
    use std::fs;
    use std::io::{BufReader, Cursor, Seek, SeekFrom};
    use std::path::Path;
    use tempfile::tempdir;

    fn get_repo_mem(key: Key) -> Repository<impl BackendClone> {
        let settings = ChunkSettings::lightweight();
        let backend = Mem::new(settings, key.clone());
        Repository::with(backend, settings, key, 2)
    }

    #[tokio::test(threaded_scheduler)]
    async fn single_add_get() {
        let seed = 0;
        println!("Seed: {}", seed);
        let chunker = FastCDC::default();

        let key = Key::random(32);
        let size = 2 * 2_usize.pow(14);
        let mut data = vec![0_u8; size];
        let mut rand = SmallRng::seed_from_u64(seed);
        rand.fill_bytes(&mut data);
        let mut repo = get_repo_mem(key);

        let mut archive = ActiveArchive::new("test");

        let testdir = tempdir().unwrap();
        let input_file_path = testdir.path().join(Path::new("file1"));
        {
            let mut input_file = fs::File::create(input_file_path.clone()).unwrap();
            input_file.write_all(&data).unwrap();
        }
        let input_file = BufReader::new(fs::File::open(input_file_path).unwrap());

        archive
            .put_object(&chunker, &mut repo, "FileOne", input_file)
            .await
            .unwrap();

        let mut buf = Cursor::new(Vec::<u8>::new());
        archive
            .get_object(&mut repo, "FileOne", &mut buf)
            .await
            .unwrap();

        let output = buf.into_inner();
        println!("Input length: {}", data.len());
        println!("Output length: {}", output.len());

        let mut mismatch = false;
        for i in 0..data.len() {
            if data[i] != output[i] {
                println!(
                    "Byte {} was different in output. Input val: {:X?} Output val {:X?}",
                    i, data[i], output[i]
                );

                mismatch = true;
            }
        }

        assert!(!mismatch);
    }

    #[tokio::test(threaded_scheduler)]
    async fn sparse_add_get() {
        let seed = 0;
        let chunker: FastCDC = FastCDC::default();
        let key = Key::random(32);
        let mut repo = get_repo_mem(key);

        let mut archive = ActiveArchive::new("test");

        let mut rng = SmallRng::seed_from_u64(seed);
        // Generate a random number of extents from one to ten
        let mut extents: Vec<Extent> = Vec::new();
        let extent_count: usize = rng.gen_range(1, 10);
        let mut next_start: u64 = 0;
        let mut final_size: usize = 0;
        for _ in 0..extent_count {
            // Each extent can be between 256 bytes and 16384 bytes long
            let extent_length = rng.gen_range(256, 16384);
            let extent = Extent {
                start: next_start,
                end: next_start + extent_length,
            };
            // Keep track of final size as we grow
            final_size = (next_start + extent_length) as usize;
            extents.push(extent);
            // Each extent can be between 256 and 16384 bytes appart
            let jump = rng.gen_range(256, 16384);
            next_start = next_start + extent_length + jump;
        }

        // Create the test data
        let mut test_input = vec![0_u8; final_size];
        // Fill the test vector with random data
        for Extent { start, end } in extents.clone() {
            for i in start..end {
                test_input[i as usize] = rng.gen();
            }
        }

        // Make the extent list
        let mut extent_list = Vec::new();
        for extent in extents.clone() {
            let data = test_input[extent.start as usize..extent.end as usize].to_vec();
            extent_list.push((extent, Cursor::new(data)));
        }

        // println!("Extent list: {:?}", extent_list);
        // Load data into archive
        archive
            .put_sparse_object(&chunker, &mut repo, "test", extent_list)
            .await
            .expect("Archive Put Failed");

        // Create output vec
        let test_output = Vec::new();
        println!("Output is a buffer of {} bytes.", final_size);
        let mut cursor = Cursor::new(test_output);
        for (i, extent) in extents.clone().iter().enumerate() {
            println!("Getting extent #{} : {:?}", i, extent);
            cursor
                .seek(SeekFrom::Start(extent.start))
                .expect("Out of bounds");
            archive
                .get_extent(&mut repo, "test", *extent, &mut cursor)
                .await
                .expect("Archive Get Failed");
        }
        let test_output = cursor.into_inner();
        println!("Input is now a buffer of {} bytes.", test_input.len());
        println!("Output is now a buffer of {} bytes.", test_output.len());

        for i in 0..test_input.len() {
            if test_output[i] != test_input[i] {
                println!("Difference at {}", i);
                println!("Orig: {:?}", &test_input[i - 2..i + 3]);
                println!("New: {:?}", &test_output[i - 2..i + 3]);
                break;
            }
        }

        std::mem::drop(repo);

        assert_eq!(test_input, test_output);
    }

    #[test]
    fn default_namespace() {
        let archive = ActiveArchive::new("test");
        let namespace = archive.canonical_namespace();
        assert_eq!(namespace, ":");
    }

    #[test]
    fn namespace_append() {
        let archive = ActiveArchive::new("test");
        let archive = archive.namespace_append("1");
        let archive = archive.namespace_append("2");
        let namespace = archive.canonical_namespace();
        println!("Namespace: {}", namespace);
        assert_eq!(namespace, "1:2:");
    }

    #[tokio::test(threaded_scheduler)]
    async fn namespaced_insertions() {
        let chunker = FastCDC::default();
        let key = Key::random(32);

        let mut repo = get_repo_mem(key);

        let obj1 = Cursor::new([1_u8; 32]);
        let obj2 = Cursor::new([2_u8; 32]);

        let mut archive_1 = ActiveArchive::new("test");
        let mut archive_2 = archive_1.clone();

        archive_1
            .put_object(&chunker, &mut repo, "1", obj1.clone())
            .await
            .unwrap();
        archive_2
            .put_object(&chunker, &mut repo, "2", obj2.clone())
            .await
            .unwrap();

        let mut restore_1 = Cursor::new(Vec::<u8>::new());
        archive_2
            .get_object(&mut repo, "1", &mut restore_1)
            .await
            .unwrap();

        let mut restore_2 = Cursor::new(Vec::<u8>::new());
        archive_1
            .get_object(&mut repo, "2", &mut restore_2)
            .await
            .unwrap();

        let obj1 = obj1.into_inner();
        let obj2 = obj2.into_inner();

        let restore1 = restore_1.into_inner();
        let restore2 = restore_2.into_inner();

        assert_eq!(&obj1[..], &restore1[..]);
        assert_eq!(&obj2[..], &restore2[..]);
    }

    #[tokio::test(threaded_scheduler)]
    async fn commit_and_load() {
        let chunker = FastCDC::default();
        let key = Key::random(32);

        let mut repo = get_repo_mem(key);
        let mut obj1 = [0_u8; 32];
        for i in 0..obj1.len() {
            obj1[i] = i as u8;
        }

        let obj1 = Cursor::new(obj1);

        let mut archive = ActiveArchive::new("test");
        archive
            .put_object(&chunker, &mut repo, "1", obj1.clone())
            .await
            .expect("Unable to put object in archive");

        let stored_archive = archive.store(&mut repo).await;

        let archive = stored_archive
            .load(&mut repo)
            .await
            .expect("Unable to load archive from repository");

        let mut obj_restore = Cursor::new(Vec::new());
        archive
            .get_object(&mut repo, "1", &mut obj_restore)
            .await
            .expect("Unable to restore object from archive");

        assert_eq!(&obj1.into_inner()[..], &obj_restore.into_inner()[..]);
    }
}
