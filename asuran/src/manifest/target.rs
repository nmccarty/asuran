pub mod filesystem;

pub use filesystem::FileSystemTarget;

use async_trait::async_trait;
use std::collections::HashMap;
use std::io::{Read, Write};

pub use asuran_core::manifest::listing::*;

/// Representation of a `Read`/`Write` for an object, and the range of bytes within
/// that object it is responsible for
pub struct ByteRange<T> {
    pub start: u64,
    pub end: u64,
    pub object: T,
}

/// A collection of `Read`s and the byte ranges that they are associated with, in an
/// object to be committed to a repository.
///
/// The `ranges` list may contain zero, one, or many ranges, in the case of an empty
/// file, a dense file, or a sparse file respectively.
pub struct BackupObject<T: Read> {
    /// The ranges of bytes that compose this object
    ranges: Vec<ByteRange<T>>,
    /// Total size of the object in bytes, including any holes
    total_size: u64,
}

impl<T: Read> BackupObject<T> {
    /// Create a new, empty `BackupObject` with a predefined total size
    pub fn new(total_size: u64) -> BackupObject<T> {
        let ranges = Vec::new();
        BackupObject { ranges, total_size }
    }

    /// Add a new range to the list
    ///
    /// TODO (#13): Store the ranges in sorted order
    pub fn add_range(&mut self, range: ByteRange<T>) {
        self.ranges.push(range);
    }

    #[cfg_attr(tarpaulin, skip)]
    /// Returns the `total_size` of the object
    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    #[cfg_attr(tarpaulin, skip)]
    /// Sets the total size of the object
    pub fn set_total_size(&mut self, total_size: u64) {
        self.total_size = total_size;
    }

    /// Returns the ranges in the object, consuming this struct
    pub fn ranges(self) -> Vec<ByteRange<T>> {
        self.ranges
    }

    /// Adds a range without the caller needing to construct the objec themself
    pub fn direct_add_range(&mut self, start: u64, end: u64, read: T) {
        self.add_range(ByteRange {
            start,
            end,
            object: read,
        });
    }
}

/// A collection of `Write`s and their associated byte ranges with in an object to
/// be restored from a repository.
///
/// The `ranges` list may contain zero, one, or many ranges, in the case of an empty
/// file, a dense file, or a sparse file, respectively
pub struct RestoreObject<T: Write> {
    /// The list of writers and extents used to restore an object
    ranges: Vec<ByteRange<T>>,
    /// Total size of the resulting object, including any holes
    total_size: u64,
}

impl<T: Write> RestoreObject<T> {
    /// Create a new, empty `RestoreObject` with a defined size
    pub fn new(total_size: u64) -> RestoreObject<T> {
        let ranges = Vec::new();
        RestoreObject { ranges, total_size }
    }

    /// Add a new range to the list
    ///
    /// TODO  (#13): Store the ranges in sorted order
    pub fn add_range(&mut self, range: ByteRange<T>) {
        self.ranges.push(range);
    }

    #[cfg_attr(tarpaulin, skip)]
    /// Returns the `total_size` of the object
    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    #[cfg_attr(tarpaulin, skip)]
    /// Sets the total size of the object
    pub fn set_total_size(&mut self, total_size: u64) {
        self.total_size = total_size;
    }

    /// Returns the ranges in the object, consuming this struct
    pub fn ranges(self) -> Vec<ByteRange<T>> {
        self.ranges
    }

    /// Adds a range without the caller needing to construct the objec themself
    pub fn direct_add_range(&mut self, start: u64, end: u64, write: T) {
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
#[async_trait]
pub trait BackupTarget<T: Read>: Clone + Send + Sync {
    /// Returns a listing of all the backup-able objects in the target's domain
    ///
    /// This function does not do anything to the internal listing, and
    async fn backup_paths(&self) -> Listing;

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
    async fn backup_object(&self, node: Node) -> HashMap<String, BackupObject<T>>;

    /// Returns a serialized listing that should be stored in an archive at
    /// archive:listing
    async fn backup_listing(&self) -> Listing;
}

/// Collection of methods that a restore target has to implement in order for a
/// generic restore driver to be able to load and properly restore its objects
/// from a repository.
///
/// As the work of restoring an archive should be split among serveral threads,
/// it is important that targets be thread-aware and thread safe.Into
#[async_trait]
pub trait RestoreTarget<T: Write>: Clone + Send + Sync {
    /// Loads an object listing and creates a new restore target from it
    async fn load_listing(root_path: &str, listing: Listing) -> Self;

    /// Returns a copy of the internal listing object
    ///
    /// This should almost always be a clone of the object you fed into load_listing
    async fn restore_listing(&self) -> Listing;

    /// Takes an object path
    ///
    /// Returns a hashmap, keyed by namespace, of the various parts of this object
    async fn restore_object(&self, path: Node) -> HashMap<String, RestoreObject<T>>;
}
