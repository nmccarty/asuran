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
    let backend =
        asuran::repository::backend::flatfile::FlatFile::new(path, Some(settings), enc_key, 4)
            .unwrap();
    Repository::with(backend, settings, key, 2)
}
