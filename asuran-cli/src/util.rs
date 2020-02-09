use anyhow::{anyhow, Result};
use asuran::repository::backend::multifile::*;
use asuran::repository::backend::*;
use asuran::repository::*;
use std::fs::canonicalize;
use std::path::Path;

/// Attempts to open the repository at the specified location in the filesystem
pub async fn open_repo_filesystem(
    repo_path: impl AsRef<Path>,
    user_key: &[u8],
    settings: Option<ChunkSettings>,
) -> Result<Repository<impl Backend>> {
    // Canonicalize the path and make sure it exists
    let path = canonicalize(repo_path.as_ref())?;
    if !Path::exists(&path) {
        Err(anyhow!("Repository path does not exist"))
    } else {
        // Open the backend, load, and decrypt the key
        // first, open the backend just to load the key
        let enc_key = MultiFile::read_key(&path)?;
        let key = enc_key.decrypt(user_key)?;
        // Open it again, for real
        let backend = MultiFile::open_defaults(&path, settings, &key)?;

        let settings = settings.unwrap_or(backend.get_manifest().chunk_settings().await);
        // Construct the repository and return it
        Ok(Repository::new(
            backend,
            settings.compression,
            settings.hmac,
            settings.encryption,
            key,
        ))
    }
}
