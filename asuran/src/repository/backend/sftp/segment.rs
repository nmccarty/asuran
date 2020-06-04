use super::util::LockedFile;
use super::SFTPConnection;
use crate::repository::backend::common::segment::Segment;
use crate::repository::backend::{BackendError, Result, SegmentDescriptor};
use crate::repository::{Chunk, ChunkSettings, Key};

use lru::LruCache;
use ssh2::File;

use std::io::{Read, Seek, Write};
use std::path::PathBuf;
use std::rc::Rc;

pub struct SegmentPair<R: Read + Write + Seek>(u64, Segment<R>);

pub struct SFTPSegmentHandler {
    /// The connection this SegmentHandler is using
    connection: SFTPConnection,
    /// The Segment we are currently writing too, if it exists
    current_segment: Option<SegmentPair<LockedFile>>,
    /// The ID of the higest segment we have encountered
    highest_segment: u64,
    /// The size limit of each segment in bytes
    ///
    /// This is currently a soft limit, segments are closed after the write in which they go over
    size_limit: u64,
    /// An LRU cache of recently used segments, opened in RO mode
    ro_segment_cache: LruCache<u64, SegmentPair<File>>,
    /// The path of the data directory
    path: PathBuf,
    /// The number of segments per directory
    segments_per_directory: u64,
    /// The chunk settings used for encrypting headers
    chunk_settings: ChunkSettings,
    /// The key used for encrypting/decrypting headers
    key: Key,
}

impl SFTPSegmentHandler {
    #[allow(clippy::filter_map)]
    pub fn connect(
        settings: impl Into<SFTPConnection>,
        size_limit: u64,
        segments_per_directory: u64,
        chunk_settings: ChunkSettings,
        key: Key,
    ) -> Result<SFTPSegmentHandler> {
        let connection = settings.into().with_connection()?;
        let sftp = connection.sftp().unwrap();
        let repository_path = PathBuf::from(&connection.settings().path);
        // Create the repository folder if it does not exist.
        if sftp.stat(&repository_path).is_err() {
            sftp.mkdir(&repository_path, 0o775)?;
        }
        let data_path = repository_path.join("data");
        // Create the data folder if it does not exist
        if sftp.stat(&data_path).is_err() {
            sftp.mkdir(&data_path, 0o775)?;
        }

        // Walk the data directory to find the highest numbered segment
        let max_segment: Option<u64> = sftp
            // Read The folders in the data directory
            .readdir(&data_path)?
            // Filter those that are folders whose names are numbers
            .into_iter()
            .filter(|(_, file_stat)| file_stat.file_type().is_dir())
            .filter(|(path, _)| {
                path.components()
                    .next_back()
                    .and_then(|x| x.as_os_str().to_string_lossy().parse::<u64>().ok())
                    .is_some()
            })
            // Read each of those directories
            .map(|(path, _)| sftp.readdir(&path))
            // Turn any errors into backend errors
            .map(|x| x.map_err(|y| y.into()))
            // Collect to stop on error
            .collect::<Result<Vec<_>>>()?
            // Bring it into an iterator over segments
            .into_iter()
            .flatten()
            // Include only files whose names are numbers
            .filter(|(_path, file_stat)| file_stat.file_type().is_file())
            .filter_map(|(path, _)| {
                path.file_name()
                    .and_then(|x| x.to_string_lossy().parse::<u64>().ok())
            })
            .max();

        let mut segment_handler = SFTPSegmentHandler {
            connection,
            current_segment: None,
            highest_segment: max_segment.unwrap_or(0),
            size_limit,
            ro_segment_cache: LruCache::new(25),
            path: data_path,
            segments_per_directory,
            chunk_settings,
            key,
        };
        // Open the writing segment, to ensure that the data directory is lockable
        segment_handler.open_segment_write()?;
        Ok(segment_handler)
    }

    pub fn open_segment_read(&mut self, segment_id: u64) -> Result<&mut SegmentPair<File>> {
        // If we were writing to the segment, flush it and discard it
        if let Some(segment) = self.current_segment.as_mut() {
            if segment.0 == segment_id {
                segment.1.flush()?;
                self.current_segment = None;
            }
        }

        // Check the cache
        // Insert the segment into the cache if it doesn't exist
        if !self.ro_segment_cache.contains(&segment_id) {
            if !self.segment_exists(segment_id) {
                return Err(BackendError::SegmentError(format!(
                    "Segment with id {} or its containing folder does not exist",
                    segment_id
                )));
            }

            let sftp = self.connection.sftp().unwrap();

            let folder_id = segment_id / self.segments_per_directory;
            let segment_path = self
                .path
                .join(folder_id.to_string())
                .join(segment_id.to_string());
            let header_path = self
                .path
                .join(folder_id.to_string())
                .join(format!("{}.header", segment_id.to_string()));
            // Open the segment
            let segment_file = sftp.open(&segment_path)?;
            let header_file = sftp.open(&header_path)?;
            // Pack it and load it in to the cache
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
            self.ro_segment_cache.put(segment_id, segment_pair);
        }

        let segment_pair = self.ro_segment_cache.get_mut(&segment_id).unwrap();
        Ok(segment_pair)
    }

    pub fn segment_exists(&self, segment_id: u64) -> bool {
        let folder_id = segment_id / self.segments_per_directory;
        // Find the folder it belongs to and check to see if it exists
        let folder_path = self.path.join(folder_id.to_string());
        let sftp = self.connection.sftp().unwrap();
        if let Ok(file_stat) = sftp.stat(&folder_path) {
            if file_stat.file_type().is_dir() {
                let segment_path = folder_path.join(segment_id.to_string());
                sftp.stat(&segment_path).is_ok()
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn open_segment_write(&mut self) -> Result<&mut SegmentPair<LockedFile>> {
        // Check to see if we already have an open segment
        if self.current_segment.is_none() {
            while self.segment_exists(self.highest_segment) {
                self.highest_segment += 1;
            }

            let sftp = self.connection.sftp().unwrap();

            // Check to see if higest numbered existing segment is lockable and not oversized
            if self.highest_segment > 0 {
                let segment_id = self.highest_segment - 1;
                let folder_id = segment_id / self.segments_per_directory;
                let folder_path = self.path.join(folder_id.to_string());
                let segment_path = folder_path.join(segment_id.to_string());
                let header_path = folder_path.join(format!("{}.header", segment_id));
                let segment_file = LockedFile::open_read_write(&segment_path, Rc::clone(&sftp))?;
                let header_file = LockedFile::open_read_write(&header_path, Rc::clone(&sftp))?;
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
                            self.ro_segment_cache.pop(&segment.0);
                            self.current_segment = Some(segment);
                            return Ok(self.current_segment.as_mut().unwrap());
                        }
                    }
                }
            }

            let segment_id = self.highest_segment;
            let folder_id = segment_id / self.segments_per_directory;
            let folder_path = self.path.join(folder_id.to_string());
            // Create folder if it does not exist
            if sftp.stat(&folder_path).is_err() {
                sftp.mkdir(&folder_path, 0o775)?;
            }
            let segment_path = folder_path.join(segment_id.to_string());
            let header_path = folder_path.join(format!("{}.header", segment_id));
            let segment_file = LockedFile::open_read_write(&segment_path, Rc::clone(&sftp))?
                .ok_or_else(|| {
                    BackendError::SegmentError(format!(
                        "Unable to lock newly created segment. File: {:?} Src File: {} Line: {}",
                        &segment_path,
                        file!(),
                        line!()
                    ))
                })?;
            let header_file = LockedFile::open_read_write(&header_path, Rc::clone(&sftp))?
                .ok_or_else(|| {
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

        // We can now safely unwrap the current segment, since we have insured its existence
        Ok(self.current_segment.as_mut().unwrap())
    }

    pub fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        let segment_id = location.segment_id;
        let segment = self.open_segment_read(segment_id)?;
        segment.1.read_chunk(location.start)
    }

    pub fn write_chunk(&mut self, chunk: Chunk) -> Result<SegmentDescriptor> {
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

    pub fn flush(&mut self) -> Result<()> {
        if let Some(segment) = self.current_segment.as_mut() {
            segment.1.flush()
        } else {
            Ok(())
        }
    }
}

impl std::fmt::Debug for SFTPSegmentHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SFTPSegmentHandler")
            .field("path", &self.path)
            .finish()
    }
}

impl Drop for SFTPSegmentHandler {
    fn drop(&mut self) {
        self.flush().unwrap()
    }
}
