use crate::repository::backend::common::files::LockedFile;
use crate::repository::backend::common::segment::Segment;
use crate::repository::backend::{BackendError, Result, SegmentDescriptor};
use crate::repository::{Chunk, ChunkSettings, Key};

use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use lru::LruCache;
use smol::block_on;
use walkdir::WalkDir;

use std::fs::{create_dir, File};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::thread;

struct SegmentPair<R: Read + Write + Seek>(u64, Segment<R>);
/// An internal struct for handling the state of the segments
///
/// Maintains a handle to the currently being written segment, and will keep it up to date as the
/// data outgrows its file size limits
///
/// Will keep a cache of handles to files being read, to decrease the number of system calls needed
///
/// # TODOs:
///
/// 1. Implement an optional ARC cache, this could be useful for speeding up restores on highly
///    duplicated datasets
/// 2. Swtich `ro_segment_cache` to an ARC
struct InternalSegmentHandler {
    /// The segment we are currently writing too, if it exists
    current_segment: Option<SegmentPair<LockedFile>>,
    /// The ID of the highest segment we have encountered
    highest_segment: u64,
    /// The size limit of each segment, in bytes
    ///
    /// At the moment, this is a soft size limit, the segment will be closed after the first write
    /// that exceeds it completes
    size_limit: u64,
    /// An LRU cache of recently used segements, opened in RO mode
    ro_segment_cache: LruCache<u64, SegmentPair<File>>,
    /// The path of the segment directory
    path: PathBuf,
    /// The number of segments per directory
    segments_per_directory: u64,
    /// The chunk settings used for encrypting headers
    chunk_settings: ChunkSettings,
    /// They key used for encrypting/decrypting headers
    key: Key,
}

impl InternalSegmentHandler {
    /// Opens up a segment handler
    ///
    /// Will create the directory if it does not exist
    ///
    /// Note: the `repository_path` is the path of the root folder of the repository, not the data
    /// folder
    ///
    /// Will default to a cache size of 100 file handles.
    ///
    /// This implementation is not thread safe, please see `SegmentHandler` for a thread safe
    /// implementation on top of this
    ///
    /// # Errors
    ///
    /// 1. The data folder does not exist and creating it failed
    ///
    /// # Panics
    ///
    /// Any filenames in the data directory contain non-utf8 characters
    ///
    /// # TODOs:
    ///
    /// 1. This function currently recursively walks the entire data directory to find the highest
    /// numbered segment, when we can skip the recursion and only inspect the highest numbered
    /// segment folder, and still have correct behavior
    fn open(
        repository_path: impl AsRef<Path>,
        size_limit: u64,
        segments_per_directory: u64,
        chunk_settings: ChunkSettings,
        key: Key,
    ) -> Result<InternalSegmentHandler> {
        // Construct the path of the data foler
        let data_path = repository_path.as_ref().join("data");
        // Create it if it does not exist
        if !data_path.exists() {
            create_dir(&data_path)?;
        }

        // Walk the data directory to find the higest numbered segment
        let max_segment = WalkDir::new(&data_path)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| {
                e.path()
                    .file_name()
                    .map(|x| String::from(x.to_string_lossy()))
            })
            .filter_map(|e| std::result::Result::ok(e.parse::<u64>()))
            .max()
            .unwrap_or(0);

        let mut segment_handler = InternalSegmentHandler {
            current_segment: None,
            highest_segment: max_segment,
            size_limit,
            ro_segment_cache: LruCache::new(100),
            path: data_path,
            segments_per_directory,
            chunk_settings,
            key,
        };

        // Open the writing segment to ensure that the data directory is lockable
        segment_handler.open_segment_write()?;

        Ok(segment_handler)
    }

    /// Open a segement for reading
    ///
    /// Since we do not syncronize reads, and modification of existing data is forbidden as long as
    /// any instance holds a valid read lock, we do not need to worry about syncronization and
    /// simply open it as a read only file handle
    ///
    /// This method will first attempt to pull the segement out of the cache, and failing that, open
    /// it the file and inser it into the cache
    ///
    /// # Errors:
    ///
    /// 1. The segment or the folder containing it does not exist
    /// 2. Some IO error (such as lack of permissions) occurs opening the file
    fn open_segement_read(&mut self, segment_id: u64) -> Result<&mut SegmentPair<File>> {
        // if the segment we are looking for happens to be the one in the write position, we can go
        // ahead and flush it and discard it
        if let Some(segment) = self.current_segment.as_mut() {
            if segment.0 == segment_id {
                segment.1.flush()?;
                self.current_segment = None;
            }
        }

        // First, check the cache for the file
        let cache = &mut self.ro_segment_cache;
        // Due to what can only be described as lifetime nonsense, instead of branching on a if let
        // Some(x) = cache.get(segment_id), we are going the route of inserting the segment into the
        // cache if it doesn't exist, and then grabbing the refrence out of it at the end, after we
        // have ensured that the cache does indeed contain the segment, that way we only need the
        // mutable refrence in one place, and the lifetimes become much eaiser to manage
        //
        // Since this implementation is not thread safe, we do not have to worry about concurrent
        // writers, so we can ensure this refrence will be valid for as long as we need it
        if !cache.contains(&segment_id) {
            // Figure out which subfolder this belongs in and construct the path of the folder
            let folder_id = segment_id / self.segments_per_directory;
            // Find the folder it belongs to and check to see if it exists
            let folder_path = self.path.join(folder_id.to_string());
            if !(folder_path.exists() && folder_path.is_dir()) {
                return Err(BackendError::SegmentError(format!(
                    "Segment directory {} for segment {} does not exist or is not a folder",
                    folder_id, segment_id
                )));
            }
            // Get the path of the segement and check to see if it exists
            let segment_path = folder_path.join(segment_id.to_string());
            let header_path = folder_path.join(format!("{}.header", segment_id.to_string()));
            if !(segment_path.exists() && segment_path.is_file()) {
                return Err(BackendError::SegmentError(format!(
                    "File for segment {} opened in read only mode does not exists",
                    segment_id
                )));
            }
            // Open the file
            let segment_file = File::open(segment_path)?;
            let header_file = File::open(header_path)?;
            // Pack it and load it into the cache
            let segment_pair = SegmentPair(
                segment_id,
                Segment::new(
                    segment_file,
                    header_file,
                    self.size_limit,
                    self.chunk_settings,
                    self.key.clone(),
                )?,
            );
            cache.put(segment_id, segment_pair);
        }

        // Get the reference and return it
        // Unwrap is safe as we have just inserted the segement if it didn't already exist.
        let segment_pair = cache.get_mut(&segment_id).unwrap();
        Ok(segment_pair)
    }

    /// Tests if a segment exists or not
    fn segment_exists(&self, segment_id: u64) -> bool {
        let folder_id = segment_id / self.segments_per_directory;
        // Find the folder it belongs to and check to see if it exists
        let folder_path = self.path.join(folder_id.to_string());
        if !(folder_path.exists() && folder_path.is_dir()) {
            return false;
        }
        // Get the path of the segement and check to see if it exists
        let segment_path = folder_path.join(segment_id.to_string());
        segment_path.exists() && segment_path.is_file()
    }

    /// Returns the currently active writing segment
    ///
    /// Will create/open a new one if there is not currently one open
    ///
    /// # Errors:
    ///
    /// 1. Some IO error prevents the creation of a new segment file
    /// 2. We need to create a new segement folder, but a file with that name exists in the data
    ///    directory
    /// 3. We need to create a new segement, but some other instance beats us to the punch and the
    ///    new name we have chosen gets created and locked while we are running
    fn open_segment_write(&mut self) -> Result<&mut SegmentPair<LockedFile>> {
        // Check to see if we have a currently open segment, and open one up if we do not
        //
        // To make the lifetime juggling eaiser, we are going much the same route as
        // open_segment_read, opening a segement and inserting it into the option if needed,
        // ensuring the option is in the Some state. We then only need to perform a mutable refrence
        // into the option in one place, and can safely perform a simple unwrap
        if self.current_segment.is_none() {
            // Other processes may be writing to the same repository, so we can not blindly trust
            // our highest segment count, so we are going to go ahead and update that before making
            // the new segment
            while self.segment_exists(self.highest_segment) {
                self.highest_segment += 1;
            }

            // First check the previous segment and return early if it is lockable
            //
            // FIXME (#46): This is a janky fix for the library creating a new data file every time you
            // open a repository with a multifile backend, this really needs to be rewritten to
            // check for the first unlocked, non-full data file
            //
            // We do, however, skip this step if there are no segments
            if self.highest_segment > 0 {
                let segment_id = self.highest_segment - 1;
                // Find the folder that the segment needs to go into, creating it if it does not exist
                let folder_id = segment_id / self.segments_per_directory;
                let folder_path = self.path.join(folder_id.to_string());
                if !folder_path.exists() {
                    create_dir(&folder_path)?;
                }
                // Construct the path for the segment proper, and construct the segment
                let segment_path = folder_path.join(segment_id.to_string());
                let header_path = folder_path.join(format!("{}.header", segment_id.to_string()));
                let segment_file = LockedFile::open_read_write(&segment_path)?;
                let header_file = LockedFile::open_read_write(&header_path)?;
                if let Some(segment_file) = segment_file {
                    if let Some(header_file) = header_file {
                        let mut segment = SegmentPair(
                            segment_id,
                            Segment::new(
                                segment_file,
                                header_file,
                                self.size_limit,
                                self.chunk_settings,
                                self.key.clone(),
                            )?,
                        );
                        if segment.1.size() < self.size_limit {
                            // If the segment is in the cache, we need to invalidate it
                            self.ro_segment_cache.pop(&segment.0);
                            self.current_segment = Some(segment);
                            return Ok(self.current_segment.as_mut().unwrap());
                        }
                    }
                }
            }

            let segment_id = self.highest_segment;
            // Find the folder that the segment needs to go into, creating it if it does not exist
            let folder_id = segment_id / self.segments_per_directory;
            let folder_path = self.path.join(folder_id.to_string());
            if !folder_path.exists() {
                create_dir(&folder_path)?;
            }
            // Construct the path for the segment proper, and construct the segment
            let segment_path = folder_path.join(segment_id.to_string());
            let header_path = folder_path.join(format!("{}.header", segment_id.to_string()));
            let segment_file = LockedFile::open_read_write(&segment_path)?.ok_or_else(|| {
                BackendError::SegmentError(format!(
                    "Unable to lock newly created segment. File: {:?} Src File: {} Line: {}",
                    &segment_path,
                    file!(),
                    line!()
                ))
            })?;
            let header_file = LockedFile::open_read_write(&header_path)?.ok_or_else(|| {
                BackendError::SegmentError(format!(
                    "Unable to lock newly created segment. File: {:?} Src File: {} Line: {}",
                    &header_path,
                    file!(),
                    line!()
                ))
            })?;
            let segment = SegmentPair(
                segment_id,
                Segment::new(
                    segment_file,
                    header_file,
                    self.size_limit,
                    self.chunk_settings,
                    self.key.clone(),
                )?,
            );
            self.current_segment = Some(segment);
        }

        // We have ensured that this option is in the Some state in the previous section of the
        // code, so we can safely unwrap
        Ok(self.current_segment.as_mut().unwrap())
    }

    /// Attempts to read a chunk from its associated segment
    fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        let segment_id = location.segment_id;
        let segment = self.open_segement_read(segment_id)?;
        segment.1.read_chunk(location.start)
    }

    /// Attempts to write a chunk
    ///
    /// Will close out the current segment if the size, after the write completes, execeds the max
    /// size
    fn write_chunk(&mut self, chunk: Chunk) -> Result<SegmentDescriptor> {
        // Write the chunk
        let segment = self.open_segment_write()?;
        let start = segment.1.write_chunk(chunk)?;
        let descriptor = SegmentDescriptor {
            segment_id: segment.0,
            start,
        };
        // If we have exceeded the max size, close out the current segment
        if segment.1.size() >= self.size_limit {
            self.current_segment.as_mut().map(|x| x.1.flush());
            self.current_segment = None
        }
        Ok(descriptor)
    }

    /// Flushes the changes to the current segment
    fn flush(&mut self) -> Result<()> {
        if let Some(segment) = self.current_segment.as_mut() {
            segment.1.flush()
        } else {
            Ok(())
        }
    }
}

enum SegmentHandlerCommand {
    ReadChunk(SegmentDescriptor, oneshot::Sender<Result<Chunk>>),
    WriteChunk(Chunk, oneshot::Sender<Result<SegmentDescriptor>>),
    Close(oneshot::Sender<()>),
}

#[derive(Clone)]
pub struct SegmentHandler {
    input: mpsc::Sender<SegmentHandlerCommand>,
    path: String,
}

///
/// # Warnings
///
/// 1. In order to ensure file locks are freed and all data is written to disk, you
///    must ensure that you call the close function on all segments before the
///    program terminates.
impl SegmentHandler {
    /// Opens a `SegmentHandler`, creating the data directory and the initial
    /// segment if it does not exist
    ///
    /// # Errors
    ///
    /// Will error if creating/locking a segment fails, such as if the user does
    /// not have access to that directory, or if any other I/O error occurs
    pub fn open(
        repository_path: impl AsRef<Path>,
        size_limit: u64,
        segments_per_directory: u64,
        chunk_settings: ChunkSettings,
        key: Key,
        queue_depth: usize,
    ) -> Result<SegmentHandler> {
        // Create the internal handler
        let mut handler = InternalSegmentHandler::open(
            repository_path,
            size_limit,
            segments_per_directory,
            chunk_settings,
            key,
        )?;
        // get the path from it
        let path = String::from(handler.path.to_string_lossy());
        // Create the communication channel and open the event processing loop in its own task
        let (input, mut output) = mpsc::channel(queue_depth);
        thread::spawn(move || {
            let mut final_ret = None;
            while let Some(command) = block_on(output.next()) {
                match command {
                    SegmentHandlerCommand::ReadChunk(location, ret) => {
                        ret.send(handler.read_chunk(location)).unwrap();
                    }
                    SegmentHandlerCommand::WriteChunk(chunk, ret) => {
                        ret.send(handler.write_chunk(chunk)).unwrap();
                    }
                    SegmentHandlerCommand::Close(ret) => {
                        handler.flush().unwrap();
                        final_ret = Some(ret);
                        break;
                    }
                }
            }
            // Make sure all internals are dropped before sending the signal to a possible close
            // call
            std::mem::drop(handler);
            std::mem::drop(output);
            if let Some(ret) = final_ret {
                ret.send(()).unwrap();
            }
        });

        Ok(SegmentHandler { input, path })
    }

    pub async fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        let (input, output) = oneshot::channel();
        self.input
            .send(SegmentHandlerCommand::ReadChunk(location, input))
            .await
            .unwrap();
        output.await.unwrap()
    }

    pub async fn write_chunk(&mut self, chunk: Chunk) -> Result<SegmentDescriptor> {
        let (input, output) = oneshot::channel();
        self.input
            .send(SegmentHandlerCommand::WriteChunk(chunk, input))
            .await
            .unwrap();
        output.await.unwrap()
    }

    pub async fn close(&mut self) {
        let (input, output) = oneshot::channel();
        self.input
            .send(SegmentHandlerCommand::Close(input))
            .await
            .unwrap();
        output.await.unwrap();
    }
}

impl std::fmt::Debug for SegmentHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SegmentHandler: {:?}", self.path)
    }
}
