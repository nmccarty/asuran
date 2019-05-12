use std::io::{Read, Seek, Write};

pub trait BackupTarget {
    /// Returns a list of object paths in the backend
    ///
    /// Paths are plaintext, "/" or "\" delimited strings of form "/path/to/object"
    /// These paths are treated like file paths, and usually will be filepaths, but
    /// are not required to represent actual file locations, instead they simply define
    /// a hierarchy of objects.
    fn paths(&self) -> &[String];

    /// Takes a path and returns a reader for the path this object represents,
    /// and an optional call back to call upon put completion,
    fn read_object(&self, path: &str) -> (Box<dyn Read>, Option<Box<dyn Fn()>>);
}

pub trait RestoreTarget {
    type Writer: Write + Seek;
    /// Takes a path and returns a writer to the object that path represents, as
    /// well as an optional closure to be called on restore
    fn write_object(&self, path: &str) -> (Self::Writer, Option<Box<dyn Fn()>>);
}
