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
pub fn get_repo_mem(key: Key) -> Repository<impl Backend> {
    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        hmac: HMAC::Blake2b,
        encryption: Encryption::new_aes256ctr(),
    };
    let backend = asuran::repository::backend::mem::Mem::new(settings);
    Repository::with(backend, settings, key)
}

#[allow(dead_code)]
pub fn get_repo_bare(path: &str, key: Key) -> Repository<impl Backend> {
    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        hmac: HMAC::Blake2b,
        encryption: Encryption::new_aes256ctr(),
    };
    let backend = asuran::repository::backend::multifile::MultiFile::open_defaults(
        path,
        Some(settings),
        &key,
    )
    .unwrap();
    Repository::with(backend, settings, key)
}

#[allow(dead_code)]
pub fn get_repo_flat(
    path: impl AsRef<Path>,
    key: Key,
    enc_key: Option<EncryptedKey>,
) -> Repository<impl Backend> {
    let settings = ChunkSettings::lightweight();
    let backend =
        asuran::repository::backend::flatfile::FlatFile::new(path, &key, Some(settings), enc_key)
            .unwrap();
    Repository::with(backend, settings, key)
}
