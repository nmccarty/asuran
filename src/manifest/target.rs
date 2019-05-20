use std::io::{Read, Seek, Write};

/// A hole in a sparse object
pub struct Hole {
    /// The start of the hole in an object.
    ///
    /// This is the offset from the start of the object where the first sparse
    /// byte in this sparse section of the object occurs.
    ///
    /// Calling seek(SeekFrom::Start(start)) should  place the cursor so the
    /// next byte read will be the first sparse byte.
    pub start: u64,
    /// The end of the hole in an object
    ///
    /// This is the offset from the start of the object where the last sparse
    /// byte in this sparse section of the object occurs.ReadSeek
    ///
    /// Calling seek(SeekFrom::Start(end)) should place the cursor so that the
    /// next byte read will be the last sparse byte.
    pub end: u64,
}

/// Combination trait of read+seek
pub trait ReadSeek: Read + Seek {}

/// Combination trait of write+seek
pub trait WriteSeek: Write + Seek {}

/// BackupTarget::backup_object can return either a Read
/// or a Read + Seek.
///
/// BackupTargets should only return Read + Seek if the object
/// that backs them implements a sparse format. The reader will
/// be packaged next to information about the objects holes
pub enum BackupObject {
    Dense {
        object: Box<dyn Read>,
    },
    Sparse {
        object: Box<dyn ReadSeek>,
        holes: Vec<Hole>,
    },
}

/// RestoreTarget::restore_object can write either densely or sparsely
///
/// The target should be able to determine which is approipiate for the given
/// situation. The Target is assumed to have access to the archive/respoistory
/// for this purpose.
///
/// RestoreTargets should only return Sparse when the data to be written
/// contains holes.
pub enum RestoreObject {
    Dense {
        object: Box<dyn Write>,
    },
    Sparse {
        object: Box<dyn WriteSeek>,
        holes: Vec<Hole>,
    },
}

pub trait BackupTarget {
    /// Returns a list of object paths in the backend
    ///
    /// Paths are plaintext, "/" or "\" delimited strings of form "/path/to/object"
    /// These paths are treated like file paths, and usually will be filepaths, but
    /// are not required to represent actual file locations, instead they simply define
    /// a hierarchy of objects.
    fn backup_paths(&self) -> &[String];

    /// Takes a path and returns a reader for the path this object represents
    fn backup_object(&self, path: &str) -> BackupObject;

    /// Runs any custom logic the target requires, should be called on each
    /// object after putting it into an archive
    fn backup_finalize(&self, path: &str);

    /// Should Be called when all objects have been backed up
    ///
    /// Does the work of cleaning up, like comitting the manifest
    fn backup_complete(&self);
}

pub trait RestoreTarget {
    /// Takes an object path, and returns a writer to it
    fn restore_object(&self, path: &str) -> RestoreObject;

    /// Runs any custom logic the target requires, should be called on each
    /// object after restoring it
    fn restore_finalize(&self, path: &str);
}
