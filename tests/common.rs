use libasuran::repository::*;

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
    let backend = libasuran::repository::backend::mem::Mem::new(settings);
    Repository::with(backend, settings, key)
}

#[allow(dead_code)]
pub fn get_repo_bare(path: &str, key: Key) -> Repository<impl Backend> {
    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        hmac: HMAC::Blake2b,
        encryption: Encryption::new_aes256ctr(),
    };
    let backend = libasuran::repository::backend::multifile::MultiFile::open_defaults(
        path,
        Some(settings),
        &key,
    )
    .unwrap();
    Repository::with(backend, settings, key)
}
