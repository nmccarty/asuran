use asuran::repository::*;

use std::path::Path;

#[allow(dead_code)]
pub fn get_bare_settings() -> ChunkSettings {
    ChunkSettings {
        compression: Compression::NoCompression,
        hmac: HMAC::Blake2b,
        encryption: Encryption::NoEncryption,
    }
}

#[allow(dead_code)]
pub fn get_repo_mem(key: Key) -> Repository<impl BackendClone> {
    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        hmac: HMAC::Blake2b,
        encryption: Encryption::new_aes256ctr(),
    };
    let backend = asuran::repository::backend::mem::Mem::new(settings, key.clone(), 4);
    Repository::with(backend, settings, key, 2)
}

#[allow(dead_code)]
pub async fn get_repo_bare(path: &str, key: Key) -> Repository<impl BackendClone> {
    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        hmac: HMAC::Blake2b,
        encryption: Encryption::new_aes256ctr(),
    };
    let backend = asuran::repository::backend::multifile::MultiFile::open_defaults(
        path,
        Some(settings),
        &key,
        4,
    )
    .await
    .unwrap();
    Repository::with(backend, settings, key, 2)
}

#[allow(dead_code)]
pub fn get_repo_flat(
    path: impl AsRef<Path>,
    key: Key,
    enc_key: Option<EncryptedKey>,
) -> Repository<impl BackendClone> {
    let settings = ChunkSettings::lightweight();
    let backend = asuran::repository::backend::flatfile::FlatFile::new(
        path,
        Some(settings),
        enc_key,
        key.clone(),
        4,
    )
    .unwrap();
    Repository::with(backend, settings, key, 2)
}

#[allow(dead_code)]
#[cfg(feature = "sftp")]
pub fn get_sftp_repo(path: impl AsRef<Path>, key: Key) -> Repository<impl BackendClone> {
    use asuran::repository::backend::sftp::*;
    use std::env;
    use std::path::PathBuf;
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
    let path = PathBuf::from("asuran/").join(path.as_ref());
    let settings = SFTPSettings {
        hostname,
        username,
        port: Some(port),
        password: Some(password),
        path: String::from(path.to_string_lossy()),
    };
    let handle =
        SFTP::connect(settings, key.clone(), Some(ChunkSettings::lightweight()), 2).unwrap();

    Repository::with(handle, ChunkSettings::lightweight(), key, 2)
}
