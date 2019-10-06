pub mod filesystem;

pub use filesystem::FileSystemTarget;

use std::collections::HashMap;
use std::io::{Read, Write};

/// Struct represening and object and a range of bytes it is responsible for
///
/// BUG: these need to be refactored into u64
pub struct ByteRange<T> {
    pub start: usize,
    pub end: usize,
    pub object: T,
}

/// A collection of readers and byte ranges associated with them, used for reading objects
/// into the repository.
///
/// The ranges list may contain zero, one, or many ranges, in the case of an empty file,
/// a dense file, or a sparse file, respectivly
pub struct BackupObject<T: Read> {
    /// The ranges of bytes that compose this object
    ranges: Vec<ByteRange<T>>,
    /// Total size of the object in bytes, including any holes
    total_size: usize,
}

impl<T: Read> BackupObject<T> {
    /// Create a new, empty BackupObject with a defined size
    pub fn new(total_size: usize) -> BackupObject<T> {
        let ranges = Vec::new();
        BackupObject { ranges, total_size }
    }

    /// Add a new range to the list
    ///
    /// TODO: Store the ranges in sorted order
    pub fn add_range(&mut self, range: ByteRange<T>) {
        self.ranges.push(range);
    }

    /// Returns the total_size of the object
    pub fn total_size(&self) -> usize {
        self.total_size
    }

    /// Sets the total size of the object
    pub fn set_total_size(&mut self, total_size: usize) {
        self.total_size = total_size;
    }

    /// Returns the ranges in the object, consuming this struct
    pub fn ranges(self) -> Vec<ByteRange<T>> {
        self.ranges
    }

    /// Adds a range without the caller needing to construct the objec themself
    pub fn direct_add_range(&mut self, start: usize, end: usize, read: T) {
        self.add_range(ByteRange {
            start,
            end,
            object: read,
        });
    }
}

/// A collection of writers and byte ranges associated with them, used for restoring
/// objects from the repository.
///
/// The ranges list may contain zero, one, or many ranges, in the case of an empty file,
/// a dense file, or a sparse file, respectivly
pub struct RestoreObject<T: Write> {
    /// The list of writers and extents used to restore an object
    ranges: Vec<ByteRange<T>>,
    /// Total size of the resulting object, including any holes
    total_size: usize,
}

impl<T: Write> RestoreObject<T> {
    /// Create a new, empty RestorepObject with a defined size
    pub fn new(total_size: usize) -> RestoreObject<T> {
        let ranges = Vec::new();
        RestoreObject { ranges, total_size }
    }

    /// Add a new range to the list
    ///
    /// TODO: Store the ranges in sorted order
    pub fn add_range(&mut self, range: ByteRange<T>) {
        self.ranges.push(range);
    }

    /// Returns the total_size of the object
    pub fn total_size(&self) -> usize {
        self.total_size
    }

    /// Sets the total size of the object
    pub fn set_total_size(&mut self, total_size: usize) {
        self.total_size = total_size;
    }

    /// Returns the ranges in the object, consuming this struct
    pub fn ranges(self) -> Vec<ByteRange<T>> {
        self.ranges
    }

    /// Adds a range without the caller needing to construct the objec themself
    pub fn direct_add_range(&mut self, start: usize, end: usize, write: T) {
        self.add_range(ByteRange {
            start,
            end,
            object: write,
        });
    }
}

/// Collection of methods that a backup driver has to implement in order for a
/// generic backup driver to be able to commit its objects to a repository
///
/// As the work of commiting objects to an archive may be split among several
/// threads, it is important that the target use a shared state among clones
/// and be tread safe
pub trait BackupTarget<T: Read>: Clone + Send + Sync {
    /// Returns a list of object paths in the backend
    ///
    /// Paths are plaintext, "/" or "\" delimited strings of form "/path/to/object"
    /// These paths are treated like file paths, and usually will be filepaths, but
    /// are not required to represent actual file locations, instead they simply define
    /// a hierarchy of objects.
    fn backup_paths(&self) -> Vec<String>;

    /// Takes a path and returns a reader for the path this object represents
    ///
    /// Returns a hash-map of namespaces and Objects to be inserted in each namespace
    ///
    /// The "raw data" for a backup target shuold be stored in the root
    /// namespace, represented here as the empty string. This is to allow
    /// almost any coherent data to be restored directly onto the filesystem
    ///
    /// Additional pieces of metatdata, such as filesystem permissions
    /// should be stored in a namespace roughly matching the path of the
    /// datastructure that represents them, e.g. filesystem:permissions:
    fn backup_object(&self, path: &str) -> HashMap<String, BackupObject<T>>;

    /// Returns a serialized listing that should be stored in an archive at
    /// archive:listing
    fn backup_listing(&self) -> Vec<u8>;
}

/// Collection of methods that a restore target has to implement in order for a
/// generic restore driver to be able to load and properly restore its objects
/// from a repository.
///
/// As the work of restoring an archive should be split among serveral threads,
/// it is important that targets be thread-aware and thread safe.Into
pub trait RestoreTarget<T: Write>: Clone + Send + Sync {
    /// Loads an object listing and creates a new restore target from it
    ///
    /// Will return None if deserializing the listing fails.
    fn load_listing(listing: &[u8]) -> Option<Self>;

    /// Takes an object path
    ///
    /// Returns a hashmap, keyed by namespace, of the various parts of this object
    fn restore_object(&self, path: &str) -> HashMap<String, RestoreObject<T>>;

    /// Provides a list of the path strings
    fn restore_listing(&self) -> Vec<String>;
}
