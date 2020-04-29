//! Provides access to a remote MultiFile repository over SFTP as if it were a local Multi-File
//! Repository
use super::Result;
use crate::repository::backend::common::sync_backend::SyncBackend;
use crate::repository::{Chunk, EncryptedKey};

pub mod index;
pub mod manifest;

use self::index::SFTPIndex;
use self::manifest::SFTPManifest;

/// Settings used for connecting to an SFTP server.
#[derive(Clone, Debug)]
pub struct SFTPSettings {
    /// Hostname of the SFTP server to connect to.
    pub hostname: String,
    /// Username of the user to connect as
    pub username: String,
    /// Password to connect with
    ///
    /// Optional, will attempt to use ssh-agent if not provided.
    pub password: Option<String>,
    /// Path of the repository on the server
    pub path: String,
}

#[derive(Debug)]
pub struct SFTP {
    manifest: SFTPManifest,
    index: SFTPIndex,
}

impl SFTP {
    pub fn connect_raw(settings: SFTPSettings) -> Result<Self> {
        let manifest = SFTPManifest::connect(settings.clone())?;
        let index = SFTPIndex::connect(settings)?;
        Ok(SFTP { manifest, index })
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
    fn write_key(&mut self, _key: EncryptedKey) -> Result<()> {
        todo!()
    }
    fn read_key(&mut self) -> Result<EncryptedKey> {
        todo!()
    }
    fn read_chunk(&mut self, _location: super::SegmentDescriptor) -> Result<Chunk> {
        todo!()
    }
    fn write_chunk(&mut self, _chunk: Chunk) -> Result<super::SegmentDescriptor> {
        todo!()
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

    fn get_backend(path: impl AsRef<str>) -> SFTP {
        let path = path.as_ref().to_string();
        SFTP::connect_raw(get_settings(path)).expect("Unable to connect to backend")
    }

    #[test]
    fn key_read_write() {
        let key = Key::random(32);
        let enc_key = EncryptedKey::encrypt_defaults(
            &key,
            Encryption::new_aes256ctr(),
            "ASecurePassword".as_bytes(),
        );

        let mut backend = get_backend("key_read_write");
        backend
            .write_key(enc_key.clone())
            .expect("Unable to write key");

        drop(backend);
        let mut backend = get_backend("key_read_write");

        let result = backend.read_key().expect("Unable to read key");
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

        let mut backend = get_backend("chunk_read_write");
        let desc = backend
            .write_chunk(chunk.clone())
            .expect("Unable to write chunk");

        drop(backend);
        let mut backend = get_backend("chunk_read_write");
        let ret_chunk = backend.read_chunk(desc).expect("Unable to read chunk");

        assert!(chunk == ret_chunk);
    }
}
