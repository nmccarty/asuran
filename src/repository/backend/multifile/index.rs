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
use futures::task::{Spawn, SpawnExt};
use rmp_serde as rmps;
use std::collections::HashMap;
use std::fs::{create_dir, read_dir, File};
use std::io::{Seek, SeekFrom};
use std::path::Path;

#[derive(Debug)]
struct InternalIndex {
    state: HashMap<ChunkID, SegmentDescriptor>,
    file: LockedFile,
    changes: Vec<IndexTransaction>,
}

impl InternalIndex {
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
    /// # Errors
    ///
    /// Will return Err if
    ///
    /// 1. The index folder does not exist and creating it failed
    /// 2. There are no unlocked index files and creating one fails
    /// 3. There is a file called "index" in the repository folder
    /// 4. Some other IO error (such as lack of permissions) occurs
    ///
    /// # TODOs
    ///
    /// 1. Return an error if deserializing a transaction fails before the end of the file is reached
    /// 2. This function can currently panic if we have to create a new index file, but someone else
    ///    that while we were parsing the transaction. Resolution for this conflict needs to be implemented.
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
}

#[derive(Clone)]
pub struct Index {
    input: mpsc::Sender<IndexCommand>,
    path: String,
}

impl Index {
    /// Opens an `InternalIndex` and spawns a task for processing it
    ///
    /// # Panics
    ///
    /// Will panic if spawining the task fails
    pub fn open(repository_path: impl AsRef<Path>, pool: impl Spawn) -> Result<Index> {
        // Open the index
        let mut index = InternalIndex::open(repository_path)?;
        // Create the communication channel and open the event processing loop in it own task
        let (input, mut output) = mpsc::channel(100);
        pool.spawn(async move {
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
                }
            }
        })
        .expect("Failed to spawn index task.");
        todo!()
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
