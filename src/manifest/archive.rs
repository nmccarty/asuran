use crate::chunker::{Chunker, Slice, SlicerSettings};
use crate::repository::{Backend, ChunkID, Repository};
use anyhow::Result;
use chrono::prelude::*;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::io::{Empty, Read, Write};
use std::sync::{Arc, RwLock};

#[cfg(feature = "profile")]
use flame::*;
#[cfg(feature = "profile")]
use flamer::flame;

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
/// Extent range
///
/// Values are 0 indexed
pub struct Extent {
    pub start: u64,
    pub end: u64,
}

/// Pointer to an archive in a repository
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct StoredArchive {
    /// The name of the archive
    name: String,
    /// Pointer the the archive metadata in the repository
    id: ChunkID,
    /// Time the archive was started it
    ///
    /// Used to prevent replay attackts
    timestamp: DateTime<FixedOffset>,
}

impl StoredArchive {
    /// Loads the archive metadata from the repository and unpacks it for use
    pub fn load(&self, repo: &Repository<impl Backend>) -> Result<Archive> {
        let bytes = repo.read_chunk(self.id)?;
        let mut de = Deserializer::new(&bytes[..]);
        let archive: Archive =
            Deserialize::deserialize(&mut de).expect("Unable to deserialize archive");
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
}

/// Location of a chunk in a file
#[derive(Serialize, Deserialize, Copy, Clone, Eq, PartialEq, Debug)]
pub struct ChunkLocation {
    id: ChunkID,
    start: u64,
    length: u64,
}

impl PartialOrd for ChunkLocation {
    fn partial_cmp(&self, other: &ChunkLocation) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ChunkLocation {
    fn cmp(&self, other: &ChunkLocation) -> Ordering {
        self.start.cmp(&other.start)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
/// An active Archive
pub struct Archive {
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
}

impl Archive {
    pub fn new(name: &str) -> Archive {
        Archive {
            name: name.to_string(),
            objects: Arc::new(RwLock::new(HashMap::new())),
            namespace: Vec::new(),
            timestamp: Local::now().with_timezone(Local::now().offset()),
        }
    }

    /// Places an object into a archive, as a whole, without regard to sparsity
    ///
    /// Will read holes as 0s
    #[cfg_attr(feature = "profile", flame)]
    pub fn put_object<R: Read>(
        &mut self,
        chunker: &Chunker<impl SlicerSettings<Empty> + SlicerSettings<R>>,
        repository: &mut Repository<impl Backend>,
        path: &str,
        from_reader: R,
    ) -> Result<()> {
        let mut locations: Vec<ChunkLocation> = Vec::new();
        let path = self.canonical_namespace() + path.trim();

        #[cfg(feature = "profile")]
        flame::start("Packing chunks");
        let settings = repository.chunk_settings();
        let key = repository.key();
        let slices = chunker.chunked_iterator(from_reader, 0, &settings, key);
        for Slice { data, start, end } in slices {
            let id = repository.write_unpacked_chunk(data)?.0;
            locations.push(ChunkLocation {
                id,
                start,
                length: end - start + 1,
            });
        }
        #[cfg(feature = "profile")]
        flame::end("Packing chunks");

        let mut objects = self
            .objects
            .write()
            .expect("Lock on Archive::objects is posioned.");

        objects.insert(path.to_string(), locations);

        Ok(())
    }

    /// Inserts a sparse object into the archive
    ///
    /// Requires that the object be pre-split into extents
    pub fn put_sparse_object<R: Read>(
        &mut self,
        chunker: &Chunker<impl SlicerSettings<Empty> + SlicerSettings<R>>,
        repository: &mut Repository<impl Backend>,
        path: &str,
        from_readers: Vec<(Extent, R)>,
    ) -> Result<()> {
        let mut locations: Vec<ChunkLocation> = Vec::new();
        let path = self.canonical_namespace() + path.trim();

        for (extent, read) in from_readers {
            let settings = repository.chunk_settings();
            let key = repository.key();
            let slices = chunker.chunked_iterator(read, 0, &settings, key);
            for Slice { data, start, end } in slices {
                let id = repository.write_unpacked_chunk(data)?.0;
                // This math works becasue extents are 0 indexed
                locations.push(ChunkLocation {
                    id,
                    start: start + extent.start,
                    length: end - start + 1,
                });
            }
        }

        let mut objects = self
            .objects
            .write()
            .expect("Lock on Archive::objects is posioned");
        objects.insert(path.to_string(), locations);

        Ok(())
    }

    /// Inserts an object into the archive without writing any bytes
    pub fn put_empty(&mut self, path: &str) {
        let locations: Vec<ChunkLocation> = Vec::new();
        let mut objects = self
            .objects
            .write()
            .expect("Lock on Archive::objects is posioned");
        objects.insert(path.to_string(), locations);
    }

    /// Retreives an object from the archive, without regard to sparsity.
    ///
    /// Will fill in holes with zeros.
    #[cfg_attr(feature = "profile", flame)]
    pub fn get_object(
        &self,
        repository: &Repository<impl Backend>,
        path: &str,
        mut restore_to: impl Write,
    ) -> Result<()> {
        let path = self.canonical_namespace() + path.trim();
        // Get chunk locations
        let objects = self
            .objects
            .read()
            .expect("Lock on Archive::objects is posioned.");
        println!("{:?}", path);
        let locations = objects.get(&path.to_string()).cloned();
        let mut locations = if let Some(locations) = locations {
            locations
        } else {
            return Ok(());
        };
        locations.sort_unstable();
        let mut last_index = locations[0].start;
        for location in locations.iter() {
            let id = location.id;
            // If a chunk is not included, fill the space inbween it and the last with zeros
            let start = location.start;
            if start > last_index + 1 {
                let zero = [0_u8];
                for _ in last_index + 1..start {
                    restore_to.write_all(&zero)?;
                }
            }
            let bytes = repository.read_chunk(id)?;

            restore_to.write_all(&bytes)?;
            last_index = start + location.length - 1;
        }

        Ok(())
    }

    /// Retrieve a single extent of an object from the repository
    ///
    /// Will write past the end of the last chunk ends after the extent
    pub fn get_extent(
        &self,
        repository: &Repository<impl Backend>,
        path: &str,
        extent: Extent,
        mut restore_to: impl Write,
    ) -> Result<()> {
        let path = self.canonical_namespace() + path.trim();
        let objects = self
            .objects
            .read()
            .expect("Lock on Archive::objects is posioned.");

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
            let bytes = repository.read_chunk(id)?;
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
    pub fn get_sparse_object(
        &self,
        repository: &Repository<impl Backend>,
        path: &str,
        mut to_writers: Vec<(Extent, impl Write)>,
    ) -> Result<()> {
        for (extent, restore_to) in to_writers.iter_mut() {
            self.get_extent(repository, path, *extent, restore_to)?;
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
    pub fn namespace_append(&self, name: &str) -> Archive {
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
    pub fn store(self, repo: &mut Repository<impl Backend>) -> StoredArchive {
        let mut bytes = Vec::<u8>::new();
        self.serialize(&mut Serializer::new(&mut bytes))
            .expect("Unable to serialize archive.");

        let id = repo
            .write_chunk(bytes)
            .expect("Unable to write archive metatdata to repository.")
            .0;

        repo.commit_index();

        StoredArchive {
            id,
            name: self.name,
            timestamp: self.timestamp,
        }
    }

    /// Provides the name of the archive
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Provides the timestamp of the archive
    pub fn timestamp(&self) -> &DateTime<FixedOffset> {
        &self.timestamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunker::slicer::fastcdc::FastCDC;
    use crate::chunker::*;
    use crate::repository::backend::filesystem::*;
    use crate::repository::compression::Compression;
    use crate::repository::encryption::Encryption;
    use crate::repository::hmac::HMAC;
    use crate::repository::Key;
    use quickcheck_macros::quickcheck;
    use rand::prelude::*;
    use std::fs;
    use std::io::{BufReader, Cursor, Empty, Seek, SeekFrom};
    use std::path::Path;
    use tempfile::tempdir;

    fn get_repo(key: Key) -> Repository<impl Backend> {
        let root_dir = tempdir().unwrap();
        let root_path = root_dir.path().display().to_string();

        let backend = FileSystem::new_test(&root_path);
        Repository::new(
            backend,
            Compression::ZStd { level: 1 },
            HMAC::Blake2b,
            Encryption::new_aes256ctr(),
            key,
        )
    }

    #[quickcheck]
    fn single_add_get(seed: u64) -> bool {
        println!("Seed: {}", seed);
        let slicer: FastCDC<Empty> = FastCDC::new_defaults();
        let chunker = Chunker::new(slicer.copy_settings());

        let key = Key::random(32);
        let size = 2 * 2_usize.pow(14);
        let mut data = vec![0_u8; size];
        let mut rand = SmallRng::seed_from_u64(seed);
        rand.fill_bytes(&mut data);
        let mut repo = get_repo(key);

        let mut archive = Archive::new("test");

        let testdir = tempdir().unwrap();
        let input_file_path = testdir.path().join(Path::new("file1"));
        {
            let mut input_file = fs::File::create(input_file_path.clone()).unwrap();
            input_file.write_all(&data).unwrap();
        }
        let mut input_file = BufReader::new(fs::File::open(input_file_path).unwrap());

        archive.put_object(&chunker, &mut repo, "FileOne", &mut input_file);

        let mut buf = Cursor::new(Vec::<u8>::new());
        archive.get_object(&mut repo, "FileOne", &mut buf);

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

        !mismatch
    }

    #[quickcheck]
    fn sparse_add_get(seed: u64) -> bool {
        let slicer: FastCDC<Empty> = FastCDC::new_defaults();
        let chunker = Chunker::new(slicer.copy_settings());
        let key = Key::random(32);
        let root_dir = tempdir().unwrap();
        let root_path = root_dir.path().display().to_string();

        let backend = FileSystem::new_test(&root_path);
        let mut repo = Repository::new(
            backend,
            Compression::ZStd { level: 1 },
            HMAC::Blake2b,
            Encryption::new_aes256ctr(),
            key,
        );
        repo.commit_index();

        let mut archive = Archive::new("test");

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
            extent_list.push((
                extent,
                &test_input[extent.start as usize..extent.end as usize],
            ));
        }

        // println!("Extent list: {:?}", extent_list);
        // Load data into archive
        archive
            .put_sparse_object(&chunker, &mut repo, "test", extent_list)
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
                .get_extent(&repo, "test", *extent, &mut cursor)
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

        test_input == test_output
    }

    #[test]
    fn default_namespace() {
        let archive = Archive::new("test");
        let namespace = archive.canonical_namespace();
        assert_eq!(namespace, ":");
    }

    #[test]
    fn namespace_append() {
        let archive = Archive::new("test");
        let archive = archive.namespace_append("1");
        let archive = archive.namespace_append("2");
        let namespace = archive.canonical_namespace();
        println!("Namespace: {}", namespace);
        assert_eq!(namespace, "1:2:");
    }

    #[test]
    fn namespaced_insertions() {
        let slicer: FastCDC<Empty> = FastCDC::new_defaults();
        let chunker = Chunker::new(slicer.copy_settings());
        let key = Key::random(32);

        let mut repo = get_repo(key);

        let mut obj1 = Cursor::new([1_u8; 32]);
        let mut obj2 = Cursor::new([2_u8; 32]);

        let mut archive_1 = Archive::new("test");
        let mut archive_2 = archive_1.clone();

        archive_1
            .put_object(&chunker, &mut repo, "1", &mut obj1)
            .unwrap();
        archive_2
            .put_object(&chunker, &mut repo, "2", &mut obj2)
            .unwrap();

        let mut restore_1 = Cursor::new(Vec::<u8>::new());
        archive_2.get_object(&repo, "1", &mut restore_1).unwrap();

        let mut restore_2 = Cursor::new(Vec::<u8>::new());
        archive_1.get_object(&repo, "2", &mut restore_2).unwrap();

        let obj1 = obj1.into_inner();
        let obj2 = obj2.into_inner();

        let restore1 = restore_1.into_inner();
        let restore2 = restore_2.into_inner();

        assert_eq!(&obj1[..], &restore1[..]);
        assert_eq!(&obj2[..], &restore2[..]);
    }

    #[test]
    fn commit_and_load() {
        let slicer: FastCDC<Empty> = FastCDC::new_defaults();
        let chunker = Chunker::new(slicer.copy_settings());
        let key = Key::random(32);

        let mut repo = get_repo(key);
        let mut obj1 = [0_u8; 32];
        for i in 0..obj1.len() {
            obj1[i] = i as u8;
        }

        let mut obj1 = Cursor::new(obj1);

        let mut archive = Archive::new("test");
        archive
            .put_object(&chunker, &mut repo, "1", &mut obj1)
            .expect("Unable to put object in archive");

        let stored_archive = archive.store(&mut repo);

        let archive = stored_archive
            .load(&repo)
            .expect("Unable to load archive from repository");

        let mut obj_restore = Cursor::new(Vec::new());
        archive
            .get_object(&repo, "1", &mut obj_restore)
            .expect("Unable to restore object from archive");

        assert_eq!(&obj1.into_inner()[..], &obj_restore.into_inner()[..]);
    }
}
