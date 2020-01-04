use anyhow::Result;
use std::fs::{remove_file, File, OpenOptions};
use std::ops::{Deref, DerefMut, Drop};
use std::path::{Path, PathBuf};

/// Wraps a file with its paired lock file.
///
/// The lock file is deleted upon dropping
pub struct LockedFile {
    file: File,
    path: PathBuf,
    lock_file_path: PathBuf,
}

impl LockedFile {
    /// Attempts to open a read/write view of the specified file
    ///
    /// This will fail if there is any existing lock on the file. Will create the file
    /// if it does not exist.
    pub fn open_read_write<T: AsRef<Path>>(path: T) -> Result<Option<LockedFile>> {
        // generate the lock file path
        let path = path.as_ref().to_path_buf();
        let lock_file_path = path.with_extension("lock");
        // Check to see if the lock file exists
        if Path::exists(&lock_file_path) {
            // Unable to return the lock, failing
            Ok(None)
        } else {
            // First, create the lock file
            OpenOptions::new()
                .create(true)
                .write(true)
                .open(&lock_file_path)?;
            // Second, open the real file
            let file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(&path)?;
            Ok(Some(LockedFile {
                file,
                path,
                lock_file_path,
            }))
        }
    }
}

impl Deref for LockedFile {
    type Target = File;
    fn deref(&self) -> &File {
        &self.file
    }
}

impl DerefMut for LockedFile {
    fn deref_mut(&mut self) -> &mut File {
        &mut self.file
    }
}

impl Drop for LockedFile {
    fn drop(&mut self) {
        // Delete the lock file
        remove_file(&self.lock_file_path).unwrap_or_else(|_| {
            panic!(
                "Unable to delete lock file for {:?}, something went wrong",
                self.path
            )
        });
    }
}
