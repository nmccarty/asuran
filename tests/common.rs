use futures::executor::ThreadPool;
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
    let backend = FileSystem::new_test(&root_path);
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

pub fn get_repo_mem(key: Key) -> Repository<impl Backend> {
    let pool = ThreadPool::new().unwrap();
    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        hmac: HMAC::Blake2b,
        encryption: Encryption::new_aes256ctr(),
    };
    let backend = libasuran::repository::backend::mem::Mem::new(settings, &pool);
    Repository::with(backend, settings, key, pool)
}
