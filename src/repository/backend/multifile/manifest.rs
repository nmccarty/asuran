#![allow(dead_code)]
use crate::manifest::StoredArchive;
use crate::repository::backend;
use crate::repository::backend::common::*;
use crate::repository::{ChunkSettings, Key};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use chrono::prelude::*;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use futures::task::{Spawn, SpawnExt};
use petgraph::Graph;
use rmp_serde as rmps;
use std::collections::{HashMap, HashSet};
use std::fs::{create_dir, read_dir, File};
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct InternalManifest {
    known_entries: HashMap<ManifestID, ManifestTransaction>,
    verified_memo_pad: HashSet<ManifestID>,
    heads: Vec<ManifestID>,
    file: LockedFile,
    key: Key,
    chunk_settings: ChunkSettings,
    path: PathBuf,
}

impl InternalManifest {
    /// Internal function for opening the manifest
    ///
    /// The manifest this creates is not thread safe, see `Manifest` for the threadsafe
    /// implementation on top of this
    ///
    /// Optionally sets the chunk settings.
    ///
    /// Will return error if this is a new repository and the chunk settings are not set
    fn open(
        repository_path: impl AsRef<Path>,
        key: &Key,
        settings: Option<ChunkSettings>,
    ) -> Result<InternalManifest> {
        // Construct the path of the manifest folder
        let manifest_path = repository_path.as_ref().join("manifest");
        // Check to see if it exists
        if Path::exists(&manifest_path) {
            // If it is a path, return failure
            return Err(anyhow!(
                "Failed to load manifest, {:?} is a file, not a directory",
                manifest_path
            ));
        } else {
            // Create the manifest directory
            create_dir(&manifest_path)?;
        }

        // Get the list of manifest files and sort them by ID
        let mut items = read_dir(&manifest_path)?
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

        // Collect all known transactions
        let mut known_entries = HashMap::new();
        for (_, file) in &items {
            // Open the file
            let mut file = File::open(file.path())?;
            // Keep deserializing transactions until we encounter an error
            while let Ok(tx) = rmps::decode::from_read::<_, ManifestTransaction>(&mut file) {
                known_entries.insert(tx.tag(), tx);
            }
        }

        let mut file = None;
        // Attempt to find an unlocked file
        for (_, f) in &items {
            let locked_file = LockedFile::open_read_write(f.path())?;
            if let Some(f) = locked_file {
                file = Some(f);
                break;
            }
        }

        // If we were unable to find an unlocked file, go ahead and make one
        let file = if let Some(file) = file {
            file
        } else {
            let id = if items.is_empty() {
                0
            } else {
                items[items.len() - 1].0 + 1
            };
            let path = manifest_path.join(id.to_string());
            LockedFile::open_read_write(path)?
                .expect("Somehow, our newly created manifest file is locked")
        };

        let chunk_settings = if let Some(chunk_settings) = settings {
            // Attempt to open the chunk settings file and update it
            let mut sfile = LockedFile::open_read_write(manifest_path.join("chunk.settings"))?
                .with_context(|| "Unable to lock chunk.settings")?;
            // Clear the file
            sfile.set_len(0)?;
            // Write our new chunksettings
            rmps::encode::write(&mut sfile, &chunk_settings)?;
            chunk_settings
        } else {
            let mut sfile = File::open(manifest_path.join("chunk.settings"))?;
            rmps::decode::from_read(&mut sfile)?
        };

        // Construct the Internal Manifest
        let mut manifest = InternalManifest {
            known_entries,
            verified_memo_pad: HashSet::new(),
            heads: Vec::new(),
            file,
            key: key.clone(),
            chunk_settings,
            path: manifest_path,
        };
        // Build the list of heads
        manifest.build_heads();
        // Verify each head
        for head in manifest.heads.clone() {
            if !manifest.verify_tx(head) {
                return Err(anyhow!(
                    "Manifest Transaction failed verification! {:?}",
                    manifest.known_entries.get(&head).unwrap()
                ));
            }
        }

        // Return the manifest
        Ok(manifest)
    }

    /// Gets the heads from a list of transactions
    fn build_heads(&mut self) {
        // Create the graph
        let mut graph: Graph<ManifestID, ()> = Graph::new();
        let mut index_map = HashMap::new();
        // Add each transaction to our map
        for (id, tx) in &self.known_entries {
            let tag = tx.tag();
            let id = graph.add_node(tag);
            index_map.insert(tag, id);
        }
        // Go through each transaction in the graph, adding an edge in the new -> old direction
        for tx in self.known_entries.values() {
            let id = index_map.get(&tx.tag()).unwrap();
            for other_tx in tx.previous_heads() {
                let other_id = index_map.get(&other_tx).unwrap();
                graph.update_edge(*id, *other_id, ());
            }
        }
        // reverse all the nodes, so they now point from old to new
        graph.reverse();
        // Find all nodes with no outgoing edges, these are our heads
        let mut heads = Vec::new();
        for (tag, id) in &index_map {
            let mut edges = graph.edges(*id);
            if edges.next() == None {
                heads.push(*tag);
            }
        }

        self.heads = heads;
    }

    /// Recursivly verifies a transaction and all its parents
    fn verify_tx(&mut self, id: ManifestID) -> bool {
        if self.verified_memo_pad.contains(&id) {
            true
        } else {
            let tx = self.known_entries.get(&id).unwrap().clone();
            if tx.verify(&self.key) {
                self.verified_memo_pad.insert(id);
                for parent in tx.previous_heads() {
                    if !self.verify_tx(*parent) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
    }

    /// Returns the last modification timestamp of the manifest
    ///
    /// Defaults to now if there are no heads
    fn last_modification(&self) -> DateTime<FixedOffset> {
        if self.heads.is_empty() {
            Local::now().with_timezone(Local::now().offset())
        } else {
            let first_head = self.known_entries.get(&self.heads[0]).unwrap();
            let mut max = first_head.timestamp();
            for id in &self.heads {
                let tx = self.known_entries.get(id).unwrap();
                if tx.timestamp() > max {
                    max = tx.timestamp()
                }
            }
            max
        }
    }

    /// Returns the default chunk settings in this manifest
    fn chunk_settings(&self) -> ChunkSettings {
        self.chunk_settings
    }

    /// Returns an iterator over the archives in this repository
    fn archive_iterator(&self) -> std::vec::IntoIter<StoredArchive> {
        let mut items = self.known_entries.values().cloned().collect::<Vec<_>>();
        items.sort_by(|a, b| a.timestamp().cmp(&b.timestamp()));
        items.reverse();
        items
            .into_iter()
            .map(StoredArchive::from)
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Sets the chunk settings
    fn write_chunk_settings(&mut self, settings: ChunkSettings) {
        let mut sfile = LockedFile::open_read_write(self.path.join("chunk.settings"))
            .unwrap()
            .unwrap();
        // Clear the file
        sfile.set_len(0).unwrap();
        // Write our new chunksettings
        rmps::encode::write(&mut sfile, &settings).unwrap();
        self.chunk_settings = settings;
    }

    /// Adds an archive to the manifest
    #[allow(clippy::needless_pass_by_value)]
    fn write_archive(&mut self, archive: StoredArchive) {
        // Create the transaction
        let tx = ManifestTransaction::new(
            &self.heads,
            archive.id(),
            archive.timestamp(),
            archive.name(),
            self.chunk_settings.hmac,
            &self.key,
        );
        // Write the transaction to the file
        let file = &mut self.file;
        file.seek(SeekFrom::End(0))
            .expect("Unable to seek within locked manifest file");
        rmps::encode::write(file, &tx).expect("Unable to write within locked manifest file");
        // Add the transaction to our entries list
        let id = tx.tag();
        self.known_entries.insert(id, tx);
        // Update our heads to only contain this transaction
        self.heads = vec![id]
    }
}

enum ManifestCommand {
    LastMod(oneshot::Sender<DateTime<FixedOffset>>),
    ChunkSettings(oneshot::Sender<ChunkSettings>),
    ArchiveIterator(oneshot::Sender<std::vec::IntoIter<StoredArchive>>),
    WriteChunkSettings(ChunkSettings, oneshot::Sender<()>),
    WriteArchive(StoredArchive, oneshot::Sender<()>),
}

/// A message-passing handle to a running manifest
///
/// # Warnings
///
/// 1. In order to ensure that file locks are freed and data is writeen properly, you must ensure
///    that your executor runs all futures to completion before your program terminates
#[derive(Clone)]
pub struct Manifest {
    input: mpsc::Sender<ManifestCommand>,
    path: String,
}

impl Manifest {
    /// Opens and reads the manifest, creating it if it does not exist
    ///
    /// Note that the repository path is the root path of the repository, not the path of the index
    /// folder.
    ///
    /// This method will create the manifest folder if it does not exist.
    ///
    /// Files whose names are not strictly base 10 integers are ignored, and will not be added to
    /// the state or written to.
    ///
    /// This method only creates the event loop, the actual manifest is created by
    /// `InternalManifest::open`
    ///
    /// This method can optinally set the chunksettings for the manifest, but it is an error to not
    /// provide chunk settings if the manifest has not been created yet
    ///
    /// # Errors
    ///
    /// Will return Err if
    ///
    /// 1. The manifest folder does not exist and creating it failed
    /// 2. There are no unlocked manifest folders and creating one fails
    /// 3. There is a file called "manifest" in the repository folder
    /// 4. Some other IO error (shuch as lack of permissions) occurs
    /// 5. The path contains non-utf8 characters
    ///
    /// # TODOs
    /// 1. Return an error if deserializing a transaciton fails before the end of the file is reached
    /// 2. This function can currently panic if we have to create a new manifest file, but someone
    ///    else creates the same file we are trying to first.
    pub fn open(
        repository_path: impl AsRef<Path>,
        chunk_settings: Option<ChunkSettings>,
        key: &Key,
        pool: impl Spawn,
    ) -> Result<Manifest> {
        let mut manifest = InternalManifest::open(repository_path.as_ref(), key, chunk_settings)?;
        let (input, mut output) = mpsc::channel(100);
        pool.spawn(async move {
            while let Some(command) = output.next().await {
                match command {
                    ManifestCommand::LastMod(ret) => {
                        ret.send(manifest.last_modification()).unwrap();
                    }
                    ManifestCommand::ChunkSettings(ret) => {
                        ret.send(manifest.chunk_settings()).unwrap();
                    }
                    ManifestCommand::ArchiveIterator(ret) => {
                        ret.send(manifest.archive_iterator()).unwrap();
                    }
                    ManifestCommand::WriteChunkSettings(settings, ret) => {
                        manifest.write_chunk_settings(settings);
                        ret.send(()).unwrap();
                    }
                    ManifestCommand::WriteArchive(archive, ret) => {
                        manifest.write_archive(archive);
                        ret.send(()).unwrap();
                    }
                }
            }
        })
        .expect("Failed to spawn the manifest task");

        Ok(Manifest {
            input,
            path: repository_path
                .as_ref()
                .join("manifest")
                .to_str()
                .unwrap()
                .to_string(),
        })
    }
}

impl std::fmt::Debug for Manifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Manifest: {:?}", self.path)
    }
}

#[async_trait]
impl backend::Manifest for Manifest {
    type Iterator = std::vec::IntoIter<StoredArchive>;
    async fn last_modification(&mut self) -> DateTime<FixedOffset> {
        let (i, o) = oneshot::channel();
        self.input.send(ManifestCommand::LastMod(i)).await.unwrap();
        o.await.unwrap()
    }
    async fn chunk_settings(&mut self) -> ChunkSettings {
        let (i, o) = oneshot::channel();
        self.input
            .send(ManifestCommand::ChunkSettings(i))
            .await
            .unwrap();
        o.await.unwrap()
    }
    async fn archive_iterator(&mut self) -> Self::Iterator {
        let (i, o) = oneshot::channel();
        self.input
            .send(ManifestCommand::ArchiveIterator(i))
            .await
            .unwrap();
        o.await.unwrap()
    }
    async fn write_chunk_settings(&mut self, settings: ChunkSettings) {
        let (i, o) = oneshot::channel();
        self.input
            .send(ManifestCommand::WriteChunkSettings(settings, i))
            .await
            .unwrap();
        o.await.unwrap()
    }
    async fn write_archive(&mut self, archive: StoredArchive) {
        let (i, o) = oneshot::channel();
        self.input
            .send(ManifestCommand::WriteArchive(archive, i))
            .await
            .unwrap();
        o.await.unwrap()
    }
    // This does nothing with this implementation
    async fn touch(&mut self) {}
}
