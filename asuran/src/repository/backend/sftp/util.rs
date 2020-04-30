use ssh2::{Error, File, OpenFlags, OpenType, Sftp};

use std::io::{Read, Seek, Write};
use std::ops::{Deref, DerefMut, Drop};
use std::path::{Path, PathBuf};
use std::rc::Rc;

type Result<T> = std::result::Result<T, Error>;

/// Wraps a remote file with its paired remote lock file
///
/// The lock file is deleted upon dropping
pub struct LockedFile {
    file: File,
    path: PathBuf,
    lock_file_path: PathBuf,
    sftp: Rc<Sftp>,
}

impl LockedFile {
    pub fn open_read_write<T: AsRef<Path>>(path: T, sftp: Rc<Sftp>) -> Result<Option<LockedFile>> {
        let path = path.as_ref().to_path_buf();
        let extension = if let Some(ext) = path.extension() {
            // FIXME: Really need to handle this in a way that doesn't panic on non unicode
            let mut ext = String::from(ext.to_string_lossy());
            ext.push_str(".lock");
            ext
        } else {
            "lock".to_string()
        };
        let lock_file_path = path.with_extension(extension);
        // Check if the lock file exsits, by attempting to open it in read only mode
        let lockfile_attempt = sftp.stat(&lock_file_path);
        if lockfile_attempt.is_ok() {
            return Ok(None);
        }
        // Lock file doesn't exist, try opening our main file
        let file = sftp.open_mode(
            &path,
            OpenFlags::READ | OpenFlags::WRITE | OpenFlags::CREATE,
            0o644,
            OpenType::File,
        )?;
        // Create the lock file
        let _lock_file = sftp.open_mode(
            &lock_file_path,
            OpenFlags::READ | OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUSIVE,
            0o644,
            OpenType::File,
        )?;

        Ok(Some(LockedFile {
            file,
            path,
            lock_file_path,
            sftp,
        }))
    }
}

impl Drop for LockedFile {
    fn drop(&mut self) {
        // Check to see if the lock file exists before doing anything
        if self.sftp.stat(&self.lock_file_path).is_ok() {
            self.sftp.unlink(&self.lock_file_path).unwrap_or_else(|_| {
                panic!(
                    "Unable to delete lock file for {:?}, something went wrong",
                    self.path
                )
            });
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

impl std::fmt::Debug for LockedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LockedFile")
            .field("path", &self.path)
            .field("lock_file_path", &self.lock_file_path)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::backend::sftp::*;
    use std::env;
    fn get_settings(path: String) -> SFTPSettings {
        let hostname = env::var_os("ASURAN_SFTP_HOSTNAME")
            .map(|x| x.into_string().unwrap())
            .expect("Server must be set");
        let username = env::var_os("ASURAN_SFTP_USER")
            .map(|x| x.into_string().unwrap())
            .unwrap_or("asuran".to_string());
        let password = env::var_os("ASURAN_SFTP_PASS")
            .map(|x| x.into_string().unwrap())
            .unwrap_or("asuran".to_string());
        let port = env::var_os("ASURAN_SFTP_PORT")
            .map(|x| x.into_string().unwrap())
            .unwrap_or("22".to_string())
            .parse::<u16>()
            .expect("Unable to parse port");

        SFTPSettings {
            hostname,
            username,
            port: Some(port),
            password: Some(password),
            path,
        }
    }

    #[test]
    fn lock_non_existant_file() {
        let mut connection: SFTPConnection =
            get_settings("asuran/lock_non_existant_file".to_string()).into();
        connection
            .connect()
            .expect("Unable to connect to sftp server");

        let locked_file =
            LockedFile::open_read_write("/asuran/non_existant_file", connection.sftp().unwrap())
                .expect("Unable to lock file");

        assert!(locked_file.is_some());
    }

    #[test]
    fn lock_existant_file() {
        let mut connection: SFTPConnection =
            get_settings("asuran/lock_existant_file".to_string()).into();
        connection
            .connect()
            .expect("Unable to connect to sftp server");

        let sftp = connection.sftp().unwrap();

        let path = PathBuf::from("asuran/lock_test_file");
        let _test_file = sftp.create(&path).expect("Unable to create testfile");

        let locked_file = LockedFile::open_read_write("asuran/lock_test_file", sftp)
            .expect("Unable to lock file");

        assert!(locked_file.is_some());
    }

    #[test]
    fn locking_locked_file_fails() {
        let mut connection: SFTPConnection =
            get_settings("asuran/lock_existant_file".to_string()).into();
        connection
            .connect()
            .expect("Unable to connect to sftp server");

        let sftp = connection.sftp().unwrap();

        let path = PathBuf::from("asuran/test_locked_file");
        let _test_file = sftp.create(&path).expect("Unable to create test file");
        let lock_path = PathBuf::from("asuran/test_locked_file.lock");
        let _test_file_lock = sftp.create(&lock_path).expect("Unable to create test lock");

        let locked_file = LockedFile::open_read_write("asuran/test_locked_file", sftp)
            .expect("I/O error attempting to lock file");

        assert!(locked_file.is_none());
    }
}
