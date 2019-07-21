use libasuran::repository::*;

pub fn get_repo(root_path: &str, key: Key) -> Repository<impl Backend> {
    let backend = FileSystem::new(&root_path);
    Repository::new(
        backend,
        Compression::ZStd { level: 1 },
        HMAC::Blake2b,
        Encryption::new_aes256ctr(),
        key,
    )
}

pub fn get_repo_bare(root_path: &str, key: Key) -> Repository<impl Backend> {
    let backend = FileSystem::new_test_1k(&root_path);
    Repository::new(
        backend,
        Compression::NoCompression,
        HMAC::Blake2b,
        Encryption::NoEncryption,
        key,
    )
}

pub fn get_bare_settings() -> ChunkSettings {
    ChunkSettings {
        compression: Compression::NoCompression,
        hmac: HMAC::Blake2b,
        encryption: Encryption::NoEncryption,
    }
}
