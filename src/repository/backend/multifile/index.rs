use crate::repository::backend;
use crate::repository::backend::common::*;
use crate::repository::backend::SegmentDescriptor;
use crate::repository::ChunkID;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use rmp_serde as rmps;
use std::collections::HashMap;
use std::fs::{create_dir, read_dir, File};
use std::io::{Seek, SeekFrom};
use std::path::Path;
use tokio::task;

#[derive(Debug)]
struct InternalIndex {
    state: HashMap<ChunkID, SegmentDescriptor>,
    file: LockedFile,
    changes: Vec<IndexTransaction>,
}

impl InternalIndex {
    /// Internal function for opening the index
    ///
    /// The index this creates is not thread safe, see `Index` for the thread safe implementation on
    /// top of this.
    fn open(repository_path: impl AsRef<Path>) -> Result<InternalIndex> {
        // construct the path of the index folder
        let index_path = repository_path.as_ref().join("index");
        // Check to see if it exists
        if Path::exists(&index_path) {
            // If it is a file, return failure
            if Path::is_file(&index_path) {
                return Err(anyhow!(
                    "Failed to load index, {:?} is a file, not a directory",
                    index_path
                ));
            }
        } else {
            // Create the index directory
            create_dir(&index_path)?;
        }
        // Create the state map
        let mut state: HashMap<ChunkID, SegmentDescriptor> = HashMap::new();

        // Get the list of files, and sort them by ID
        let mut items = read_dir(&index_path)?
            .filter_map(Result::ok)
            .filter(|x| x.path().is_file())
            .filter_map(|x| {
                x.path()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .map(|y| Result::ok(y.parse::<usize>()))
                    .flatten()
                    .map(|z| (z, x))
            })
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.0.cmp(&b.0));

        // Add all the seen transactions to our state hashmap
        for (_, file) in &items {
            // Open the file
            let mut file = File::open(file.path())?;
            // Keep deserializing transactions until we encouter an error
            while let Ok(tx) = rmps::decode::from_read::<_, IndexTransaction>(&mut file) {
                // Insert each item into the state
                state.insert(tx.chunk_id, tx.descriptor);
            }
        }

        // Check to see if there are any unlocked index files, and if so, use the first ones
        for (_, file) in &items {
            let locked_file = LockedFile::open_read_write(file.path())?;
            if let Some(file) = locked_file {
                return Ok(InternalIndex {
                    state,
                    file,
                    changes: Vec::new(),
                });
            }
        }

        // If we have gotten here there are no unlocked index files, creating one

        // Check the length of the items list, if it is empty, there are no index files,
        // so we must create the first
        let id = if items.is_empty() {
            0
        } else {
            items[items.len() - 1].0 + 1
        };

        let path = index_path.join(id.to_string());
        let file = LockedFile::open_read_write(path)?
            .expect("Somehow, our newly created index file is locked.");
        Ok(InternalIndex {
            state,
            file,
            changes: Vec::new(),
        })
    }

    fn drain_changes(&mut self) -> Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        for tx in self.changes.drain(0..self.changes.len()) {
            rmps::encode::write(&mut self.file, &tx)?;
        }
        Ok(())
    }
}

enum IndexCommand {
    Lookup(ChunkID, oneshot::Sender<Option<SegmentDescriptor>>),
    Set(ChunkID, SegmentDescriptor, oneshot::Sender<Result<()>>),
    Commit(oneshot::Sender<Result<()>>),
    Count(oneshot::Sender<usize>),
    Close(oneshot::Sender<()>),
}

#[derive(Clone)]
pub struct Index {
    input: mpsc::Sender<IndexCommand>,
    path: String,
}

/// Multi file index with lock free multithreading
///
/// # Warnings
///
/// 1. In order to ensure locks are freed, you must ensure that your executor runs all futures to
///    completion before your program terminates
/// 2. You must call `commit_index` for your changes to be commited to disk, the Index will not do
///    this for you
impl Index {
    /// Opens and reads the index, creating it if it does not exist.
    ///
    /// Note that the repository path is the root path of the repository, not the path of the index
    /// folder.
    ///
    /// This method will create the index folder if it does not exist.
    ///
    /// Files whos names are not strictly base 10 integers are ignored, and will not be added to the
    /// state or written to.
    ///
    /// This method only creates the event loop on its own, the actual index is created by
    /// `InternalIndex::open`
    ///
    /// # Errors
    ///
    /// Will return Err if
    ///
    /// 1. The index folder does not exist and creating it failed
    /// 2. There are no unlocked index files and creating one fails
    /// 3. There is a file called "index" in the repository folder
    /// 4. Some other IO error (such as lack of permissions) occurs
    /// 5. The path contains non-utf8 characters
    ///
    /// # TODOs
    ///
    /// 1. Return an error if deserializing a transaction fails before the end of the file is reached
    /// 2. This function can currently panic if we have to create a new index file, but someone else
    ///    that while we were parsing the transaction. Resolution for this conflict needs to be
    ///    implemented.
    pub fn open(repository_path: impl AsRef<Path>) -> Result<Index> {
        // Open the index
        let mut index = InternalIndex::open(&repository_path)?;
        // Create the communication channel and open the event processing loop in it own task
        let (input, mut output) = mpsc::channel(100);
        task::spawn(async move {
            let mut final_ret = None;
            while let Some(command) = output.next().await {
                match command {
                    IndexCommand::Lookup(id, ret) => {
                        ret.send(index.state.get(&id).copied()).unwrap();
                    }
                    IndexCommand::Set(id, descriptor, ret) => {
                        // TODO: dont insert the item into the changes list if it its already in the index
                        index.state.insert(id, descriptor);
                        let transaction = IndexTransaction {
                            chunk_id: id,
                            descriptor,
                        };
                        index.changes.push(transaction);
                        ret.send(Ok(())).unwrap();
                    }
                    IndexCommand::Count(ret) => {
                        ret.send(index.state.len()).unwrap();
                    }
                    IndexCommand::Commit(ret) => {
                        ret.send({ index.drain_changes() }).unwrap();
                    }
                    IndexCommand::Close(ret) => {
                        final_ret = Some(ret);
                        break;
                    }
                }
            }
            // Make sure that our internals are dropped before sending the completion signal to a
            // possible close call
            std::mem::drop(index);
            std::mem::drop(output);
            if let Some(ret) = final_ret {
                ret.send(()).unwrap();
            };
        });

        Ok(Index {
            input,
            path: repository_path.as_ref().to_str().unwrap().to_string(),
        })
    }

    pub async fn close(&mut self) {
        let (tx, rx) = oneshot::channel();
        self.input.send(IndexCommand::Close(tx)).await.unwrap();
        rx.await.unwrap();
    }
}

impl std::fmt::Debug for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Index: {:?}", self.path)
    }
}

#[async_trait]
impl backend::Index for Index {
    async fn lookup_chunk(&mut self, id: ChunkID) -> Option<SegmentDescriptor> {
        let (input, output) = oneshot::channel();
        self.input
            .send(IndexCommand::Lookup(id, input))
            .await
            .unwrap();
        output.await.unwrap()
    }
    async fn set_chunk(&mut self, id: ChunkID, location: SegmentDescriptor) -> Result<()> {
        let (input, output) = oneshot::channel();
        self.input
            .send(IndexCommand::Set(id, location, input))
            .await
            .unwrap();
        output.await.unwrap()
    }
    async fn commit_index(&mut self) -> Result<()> {
        let (input, output) = oneshot::channel();
        self.input.send(IndexCommand::Commit(input)).await.unwrap();
        output.await.unwrap()
    }
    async fn count_chunk(&mut self) -> usize {
        let (input, output) = oneshot::channel();
        self.input.send(IndexCommand::Count(input)).await.unwrap();
        output.await.unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use backend::Index as OtherIndex;
    use rand;
    use rand::prelude::*;
    use std::path::PathBuf;
    use tempfile::{tempdir, TempDir};
    use walkdir::WalkDir;

    // Utility function, gets a tempdir, its path, an executor, and a spawner
    fn setup() -> (TempDir, PathBuf) {
        let tempdir = tempdir().unwrap();
        let path = tempdir.path().to_path_buf();
        (tempdir, path)
    }

    // Test to make sure creating an index in an empty folder
    // 1. Doesn't Panic or error
    // 2. Creates the index directory
    // 3. Creates the initial index file (index/0)
    // 4. Locks the initial index file (index/0.lock)
    #[tokio::test]
    async fn creation_works() {
        let (tempdir, path) = setup();
        // Create the index
        let index = Index::open(&path).expect("Index creation failed");
        // Walk the directory and print some debugging info
        for entry in WalkDir::new(&path) {
            let entry = entry.unwrap();
            println!("{}", entry.path().display());
        }
        // Check for the index directory
        let index_dir = path.join("index");
        assert!(index_dir.exists());
        assert!(index_dir.is_dir());
        // Check for the initial index file
        let index_file = index_dir.join("0");
        assert!(index_file.exists());
        assert!(index_file.is_file());
        // Check for the initial index lock file
        let index_lock = index_dir.join("0.lock");
        assert!(index_lock.exists());
        assert!(index_lock.is_file());
    }

    // Test to make sure creating a second index while the first is open
    // 1. Doesn't panic or error
    // 2. Creates and locks a second index file
    #[tokio::test]
    async fn double_creation_works() {
        let (tempdir, path) = setup();
        // Create the first index
        let index1 = Index::open(&path).expect("Index 1 creation failed");
        let index2 = Index::open(&path).expect("Index 2 creation failed");
        // Walk the directory and print some debugging info
        for entry in WalkDir::new(&path) {
            let entry = entry.unwrap();
            println!("{}", entry.path().display());
        }
        // Get index dir and check for index files
        let index_dir = path.join("index");
        let if1 = index_dir.join("0");
        let if2 = index_dir.join("1");
        let il1 = index_dir.join("0.lock");
        let il2 = index_dir.join("1.lock");
        assert!(if1.exists() && if1.is_file());
        assert!(if2.exists() && if2.is_file());
        assert!(il1.exists() && il1.is_file());
        assert!(il2.exists() && il2.is_file());
    }

    // Test to make sure that dropping an Index unlocks the index file
    // Note: since we are using a single threaded executor, we must manually run all tasks to
    // completion.
    #[tokio::test]
    async fn unlock_on_drop() {
        let (tempdir, path) = setup();
        // Open an index and drop it
        let mut index = Index::open(&path).expect("Index creation failed");
        index.close().await;
        // check for the index file and the absense of the lock file
        let index_dir = path.join("index");
        let index_file = index_dir.join("0");
        let index_lock = index_dir.join("0.lock");
        assert!(index_file.exists() && index_file.is_file());
        assert!(!index_lock.exists());
    }

    // Test to verify that:
    // 1. Writing to a properly setup index does not Err or Panic
    // 2. Reading keys we have inserted into a properly setup index does not Err or Panic
    // 3. Keys are still present in the index after dropping and reloading from the same directory
    // 4. Chunk count increments properly
    #[tokio::test]
    async fn write_drop_read() {
        let (tempdir, path) = setup();
        // Get some transactions to write to the repository
        let mut txs = HashMap::new();
        for _ in 0..10 {
            let mut raw_id = [0_u8; 32];
            rand::thread_rng().fill_bytes(&mut raw_id);
            let segment_id: u64 = rand::thread_rng().gen();
            let start: u64 = rand::thread_rng().gen();
            let chunk_id = ChunkID::new(&raw_id);
            let descriptor = SegmentDescriptor { segment_id, start };
            txs.insert(chunk_id, descriptor);
        }
        // Open the index
        let mut index = Index::open(&path).expect("Index creation failed");
        // Insert the transactions
        for (id, desc) in &txs {
            index
                .set_chunk(*id, *desc)
                .await
                .expect("Adding transaction failed");
        }
        // Commit the index
        index.commit_index().await.expect("commiting index failed");
        // Get the chunk count and check it
        let count = index.count_chunk().await;
        assert_eq!(count, txs.len());
        // Drop the index and let it complete
        index.close().await;
        // Load the index back up
        let mut index = Index::open(&path).expect("Index recreation failed");
        // Walk the directory and print some debugging info
        for entry in WalkDir::new(&path) {
            let entry = entry.unwrap();
            println!("{}", entry.path().display());
        }
        // Verify we still have the same number of chunks
        let count = index.count_chunk().await;
        assert_eq!(count, txs.len());
        // Confirm that each tx is in the index and has the correct value
        for (id, desc) in txs {
            let location = index.lookup_chunk(id).await.expect("Tx retrieve failed");
            assert_eq!(desc, location);
        }
    }
}
