//! Provides access to a remote `MultiFile` repository over SFTP as if it were a local Multi-File
//! Repository
use super::{BackendError, Result, SegmentDescriptor};
use crate::repository::backend::common::sync_backend::{BackendHandle, SyncBackend, SyncManifest};
use crate::repository::{Chunk, ChunkSettings, EncryptedKey, Key};

use rmp_serde as rmps;
use ssh2::{Session, Sftp};

use std::fmt::Debug;
use std::net::TcpStream;
use std::path::PathBuf;
use std::rc::Rc;

pub mod index;
pub mod manifest;
pub mod segment;
pub mod util;

use self::index::SFTPIndex;
use self::manifest::SFTPManifest;
use self::segment::SFTPSegmentHandler;
use self::util::LockedFile;

// Allow our result type to accept the ssh2 errors easily
// Maps to `BackendError::ConnectionError(error.to_string())`
impl From<ssh2::Error> for BackendError {
    fn from(error: ssh2::Error) -> Self {
        BackendError::ConnectionError(format!("libssh2 Error: {}", error))
    }
}

/// Settings used for connecting to an SFTP server.
#[derive(Clone, Debug)]
pub struct SFTPSettings {
    /// Hostname of the SFTP server to connect to.
    pub hostname: String,
    /// Optional port to connect to, will default to 22
    pub port: Option<u16>,
    /// Username of the user to connect as
    pub username: String,
    /// Password to connect with
    ///
    /// Optional, will attempt to use ssh-agent if not provided.
    pub password: Option<String>,
    /// Path of the repository on the server
    pub path: String,
}

#[derive(Clone)]
pub enum SFTPConnection {
    Connected {
        settings: SFTPSettings,
        session: Session,
        sftp: Rc<Sftp>,
    },
    NotConnected {
        settings: SFTPSettings,
    },
}

impl SFTPConnection {
    /// Returns `true` if this `SFTPConnection` is in a connected state
    pub fn connected(&self) -> bool {
        match self {
            SFTPConnection::Connected { .. } => true,
            SFTPConnection::NotConnected { .. } => false,
        }
    }
    /// Connects to the backend if needed
    pub fn connect(&mut self) -> Result<()> {
        if self.connected() {
            Ok(())
        } else {
            let hostname: &str = &self.settings().hostname;
            let port = self.settings().port.unwrap_or(22);
            // Connect to the SSH server
            let tcp = TcpStream::connect((hostname, port))?;
            // Open up a session
            let mut session = Session::new()?;
            session.set_tcp_stream(tcp);
            session.handshake()?;
            // Attempt to authenticate with the ssh agent
            let result = session.userauth_agent(&self.settings().username);

            if result.is_err() {
                // Grab the password
                let password = self.settings().password.as_ref().ok_or_else(|| {
                    BackendError::ConnectionError(
                        format!(
                            "SFTP connection using ssh agent to {}@{}:{} failed, and no password was provided.",
                            self.settings().username,
                            hostname,
                            port)
                    )
                })?;
                // Attempt connecting with username/password
                session.userauth_password(&self.settings().username, password)?;
            }
            // If we are here and not authenticated, something is horribly wrong
            assert!(session.authenticated());

            // Open an SFTP connection
            let sftp = session.sftp()?;

            // FIXME: This is not a high performance impact issue, since this method should only
            // really be reached once per repository action, but right now I am sort of relying on
            // rustc/llvm to be smart enough to optimize out this clone.

            let new_settings = self.settings().clone();

            *self = SFTPConnection::Connected {
                settings: new_settings,
                session,
                sftp: Rc::new(sftp),
            };
            Ok(())
        }
    }
    /// Connects to the backend if needed and converts to `SFTPConnection::Connected`, otherwise
    /// returns `self` unaltered
    pub fn with_connection(mut self) -> Result<Self> {
        if self.connected() {
            Ok(self)
        } else {
            self.connect()?;
            Ok(self)
        }
    }

    /// Provides a reference to the internal settings of this connection
    pub fn settings(&self) -> &SFTPSettings {
        match self {
            SFTPConnection::Connected { settings, .. }
            | SFTPConnection::NotConnected { settings } => &settings,
        }
    }

    /// Provides a reference to the ssh session, or None if this connection is not in a connected
    /// state
    pub fn session(&self) -> Option<&Session> {
        match self {
            SFTPConnection::Connected { session, .. } => Some(&session),
            SFTPConnection::NotConnected { .. } => None,
        }
    }

    /// Provides a reference to the sftp session, or None if this connection is not in a connected
    /// state
    pub fn sftp(&self) -> Option<Rc<Sftp>> {
        match self {
            SFTPConnection::Connected { sftp, .. } => Some(Rc::clone(sftp)),
            SFTPConnection::NotConnected { .. } => None,
        }
    }
}

impl From<SFTPSettings> for SFTPConnection {
    fn from(settings: SFTPSettings) -> Self {
        SFTPConnection::NotConnected { settings }
    }
}

impl Debug for SFTPConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SFTPConnection::Connected { settings, .. } => f
                .debug_struct("SFTPConnection::Connected")
                .field("settings", settings)
                .finish(),
            SFTPConnection::NotConnected { settings } => f
                .debug_struct("SFTPConnection::NotConnected")
                .field("settings", settings)
                .finish(),
        }
    }
}

#[derive(Debug)]
pub struct SFTP {
    manifest: SFTPManifest,
    index: SFTPIndex,
    segment_handler: SFTPSegmentHandler,
    connection: SFTPConnection, // MUST be dropped last for safety with the C FFI in `ssh2`
}

impl SFTP {
    pub fn connect_raw(
        settings: impl Into<SFTPConnection>,
        key: &Key,
        chunk_settings: Option<ChunkSettings>,
    ) -> Result<Self> {
        let connection = settings.into().with_connection()?;
        let mut manifest = SFTPManifest::connect(connection.clone(), key, chunk_settings)?;
        let index = SFTPIndex::connect(connection.clone())?;
        let chunk_settings = manifest.chunk_settings();
        let size_limit = 2_000_000_000;
        let segments_per_directory = 100;
        let segment_handler = SFTPSegmentHandler::connect(
            connection.clone(),
            size_limit,
            segments_per_directory,
            chunk_settings,
            key.clone(),
        )?;

        Ok(SFTP {
            connection,
            manifest,
            index,
            segment_handler,
        })
    }

    pub fn connect(
        settings: SFTPSettings,
        key: Key,
        chunk_settings: Option<ChunkSettings>,
        queue_depth: usize,
    ) -> Result<BackendHandle<SFTP>> {
        use crossbeam_channel::bounded;
        let (s, r) = bounded(1);
        let handle = BackendHandle::new(queue_depth, move || {
            let result = Self::connect_raw(settings, &key, chunk_settings);
            match result {
                Ok(backend) => {
                    s.send(None).unwrap();
                    backend
                }
                Err(e) => {
                    s.send(Some(e)).unwrap();
                    panic!("Opening an SFTP Backend Handle Failed")
                }
            }
        });
        let error = r
            .recv()
            .expect("Backend Handle thread died before it could send us its result");

        if let Some(error) = error {
            Err(error)
        } else {
            Ok(handle)
        }
    }

    pub fn read_key<S>(settings: S) -> Result<EncryptedKey>
    where
        S: Into<SFTPConnection>,
    {
        let connection = settings.into().with_connection()?;
        let sftp = connection.sftp().unwrap();
        let key_path = PathBuf::from(&connection.settings().path).join("key");
        let key_path = sftp.realpath(&key_path).map_err(|e| {
            BackendError::ConnectionError(format!(
                "Failed to resolve path of key file at: {:?}, Error was: {}",
                key_path, e
            ))
        })?;
        let file = sftp.open(&key_path).map_err(|e| {
            BackendError::ConnectionError(format!(
                "Failed to open key file at: {:?} Error was: {}",
                key_path, e
            ))
        })?;
        Ok(rmps::decode::from_read(file)?)
    }
}

impl SyncBackend for SFTP {
    type SyncManifest = SFTPManifest;
    type SyncIndex = SFTPIndex;
    fn get_index(&mut self) -> &mut Self::SyncIndex {
        &mut self.index
    }
    fn get_manifest(&mut self) -> &mut Self::SyncManifest {
        &mut self.manifest
    }
    fn write_key(&mut self, key: EncryptedKey) -> Result<()> {
        let key_path = PathBuf::from(&self.connection.settings().path).join("key");
        let sftp = self.connection.sftp().expect("Somehow not connected");
        let mut file =
            LockedFile::open_read_write(&key_path, sftp)?.ok_or(BackendError::FileLockError)?;

        rmps::encode::write(&mut file, &key)?;
        Ok(())
    }
    fn read_key(&mut self) -> Result<EncryptedKey> {
        let key_path = PathBuf::from(&self.connection.settings().path).join("key");
        let sftp = self.connection.sftp().expect("Somehow not connected");
        let file = sftp.open(&key_path)?;
        Ok(rmps::decode::from_read(file)?)
    }
    fn read_chunk(&mut self, location: SegmentDescriptor) -> Result<Chunk> {
        self.segment_handler.read_chunk(location)
    }
    fn write_chunk(&mut self, chunk: Chunk) -> Result<SegmentDescriptor> {
        self.segment_handler.write_chunk(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{Compression, HMAC};
    use crate::repository::{Encryption, Key};
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
    fn sftp_connect() {
        let mut connection: SFTPConnection = get_settings("sftp_connect".to_string()).into();
        connection
            .connect()
            .expect("Unable to make SFTP connection");
    }

    #[test]
    fn sftp_handle_connect() {
        let settings = get_settings("asuran/handle_connect".to_string());
        let handle = SFTP::connect(
            settings,
            Key::random(32),
            Some(ChunkSettings::lightweight()),
            2,
        );
        assert!(handle.is_ok())
    }

    fn get_backend(path: impl AsRef<str>, key: &Key) -> SFTP {
        let path = path.as_ref().to_string();
        SFTP::connect_raw(get_settings(path), key, Some(ChunkSettings::lightweight()))
            .expect("Unable to connect to backend")
    }

    #[test]
    fn key_read_write() {
        let key = Key::random(32);
        let enc_key = EncryptedKey::encrypt_defaults(
            &key,
            Encryption::new_aes256ctr(),
            "ASecurePassword".as_bytes(),
        );

        let mut backend = get_backend("asuran/key_read_write", &key);
        backend
            .write_key(enc_key.clone())
            .expect("Unable to write key");

        drop(backend);
        let mut backend = get_backend("asuran/key_read_write", &key);

        let result = backend.read_key().expect("Unable to read key");
        let dec_result = result.decrypt("ASecurePassword".as_bytes()).unwrap();
        assert!(key == dec_result);

        let connection = backend.connection;
        let result = SFTP::read_key(connection).expect("Unable to read key");
        let dec_result = result.decrypt("ASecurePassword".as_bytes()).unwrap();
        assert!(key == dec_result);
    }

    #[test]
    fn chunk_read_write() {
        let key = Key::random(32);
        let chunk = Chunk::pack(
            vec![1_u8; 1024],
            Compression::NoCompression,
            Encryption::NoEncryption,
            HMAC::Blake3,
            &key,
        );

        let mut backend = get_backend("asuran/chunk_read_write", &key);
        let desc = backend
            .write_chunk(chunk.clone())
            .expect("Unable to write chunk");

        drop(backend);
        let mut backend = get_backend("asuran/chunk_read_write", &key);
        let ret_chunk = backend.read_chunk(desc).expect("Unable to read chunk");

        assert!(chunk == ret_chunk);
    }

    // Connecting without a password or valid ssh-agent credentials should fail
    #[test]
    fn connection_fails() {
        let hostname = env::var_os("ASURAN_SFTP_HOSTNAME")
            .map(|x| x.into_string().unwrap())
            .expect("Server must be set");
        let username = env::var_os("ASURAN_SFTP_USER")
            .map(|x| x.into_string().unwrap())
            .unwrap_or("asuran".to_string());
        let port = env::var_os("ASURAN_SFTP_PORT")
            .map(|x| x.into_string().unwrap())
            .unwrap_or("22".to_string())
            .parse::<u16>()
            .expect("Unable to parse port");

        let settings = SFTPSettings {
            hostname,
            username,
            port: Some(port),
            password: None,
            path: "OhNo!".to_string(),
        };

        let connection: SFTPConnection = settings.into();

        let result = connection.with_connection();

        assert!(matches!(result, Err(BackendError::ConnectionError(_))));
    }

    // A not connected connection should return none, and a connected one should return Some
    #[test]
    fn get_session() {
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

        let settings = SFTPSettings {
            hostname,
            username,
            port: Some(port),
            password: Some(password),
            path: "yes".to_string(),
        };

        let connection: SFTPConnection = settings.into();
        assert!(connection.session().is_none());
        let connection = connection.with_connection().unwrap();
        assert!(connection.session().is_some());
    }
}
