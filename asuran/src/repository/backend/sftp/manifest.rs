use super::SFTPSettings;
use crate::repository::backend::common::sync_backend::SyncManifest;
use crate::repository::ChunkSettings;
use crate::{manifest::StoredArchive, repository::backend::Result};

#[derive(Debug)]
pub struct SFTPManifest {}

impl SFTPManifest {
    /// Will attempt to open or create a manifest at the location pointed to by the path variable of
    /// the given settings at the given server
    pub fn connect(_settings: SFTPSettings) -> Result<Self> {
        todo!()
    }
}

impl SyncManifest for SFTPManifest {
    type Iterator = std::vec::IntoIter<StoredArchive>;
    fn last_modification(&mut self) -> Result<chrono::DateTime<chrono::FixedOffset>> {
        todo!()
    }
    fn chunk_settings(&mut self) -> ChunkSettings {
        todo!()
    }
    fn archive_iterator(&mut self) -> Self::Iterator {
        todo!()
    }
    fn write_chunk_settings(&mut self, _settings: ChunkSettings) -> Result<()> {
        todo!()
    }
    fn write_archive(&mut self, _archive: StoredArchive) -> Result<()> {
        todo!()
    }
    fn touch(&mut self) -> Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
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

    fn get_manifest(path: impl AsRef<str>) -> SFTPManifest {
        let path = path.as_ref().to_string();
        SFTPManifest::connect(get_settings(path)).expect("Unable to connect to manifest")
    }

    #[test]
    fn connect() {
        get_manifest("manifest-connect");
    }

    #[test]
    fn chunk_settings() {
        let mut manifest = get_manifest("manifest-chunk settings");
        let settings = ChunkSettings::lightweight();
        manifest
            .write_chunk_settings(settings)
            .expect("Unable to write chunk settings");
        drop(manifest);
        let mut manifest = get_manifest("manifest-chunksettings");
        let new_settings = manifest.chunk_settings();
        assert!(settings == new_settings)
    }

    #[test]
    fn touch_and_modification() {
        let mut manifest = get_manifest("manifest_touch");
        manifest.touch().unwrap();
        let x = manifest.last_modification().unwrap();
        drop(manifest);
        let mut manifest = get_manifest("manifest_touch");
        std::thread::sleep(std::time::Duration::from_secs(2));
        manifest.touch().unwrap();
        drop(manifest);
        let mut manifest = get_manifest("manifest_touch");
        let y = manifest.last_modification().unwrap();
        assert!(x != y)
    }

    #[test]
    fn archives() {
        let dummy_archives: HashSet<StoredArchive> =
            (0..10).map(|_| StoredArchive::dummy_archive()).collect();

        let mut manifest = get_manifest("manifest_archives");

        for archive in &dummy_archives {
            manifest
                .write_archive(archive.clone())
                .expect("Unable to write archive");
        }

        drop(manifest);
        let mut manifest = get_manifest("manifest_archives");

        let output: HashSet<StoredArchive> = manifest.archive_iterator().collect();

        assert!(dummy_archives == output);
    }
}
