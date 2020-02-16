use std::fs::{remove_file, File, OpenOptions};
use std::io::{Read, Result, Seek, Write};
use std::ops::{Deref, DerefMut, Drop};
use std::path::{Path, PathBuf};

/// Wraps a file with its paired lock file.
///
/// The lock file is deleted upon dropping
#[derive(Debug)]
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
        // Check to see if the lock file exists before doing anything, if it is already gone (i.e.
        // if it was in a now dropped tempdir) we dont need to do anything
        if self.lock_file_path.exists() {
            // Delete the lock file
            remove_file(&self.lock_file_path).unwrap_or_else(|_| {
                panic!(
                    "Unable to delete lock file for {:?}, something went wrong",
                    self.path
                )
            });
        }
    }
}

impl Read for LockedFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl Write for LockedFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl Seek for LockedFile {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.file.seek(pos)
    }
}
