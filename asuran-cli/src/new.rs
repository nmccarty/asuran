use crate::cli::{Opt, RepositoryType};

use asuran::repository::backend::flatfile::FlatFile;
use asuran::repository::backend::multifile::MultiFile;
use asuran::repository::backend::Backend;
use asuran::repository::{EncryptedKey, Key};

use anyhow::{anyhow, Context, Result};

use std::fs::create_dir_all;

/// Creates a new repository with the user specified settings ad the user
/// specified location
pub async fn new(options: Opt) -> Result<()> {
    // Ensure that the repository path does not exist
    if options.repo_opts().repo.exists() {
        return Err(anyhow!(
            "Repository location already exists! {:?}",
            &options.repo_opts().repo
        ));
    }

    // Figure out what encryption type the user wants to use and get the encryption length
    let settings = options.get_chunk_settings();
    let key_length = settings.encryption.key_length();
    // Make them a new random key
    let key = Key::random(key_length);
    // Attempt to encrypt that key with the user supplied password
    let encrypted_key = EncryptedKey::encrypt_defaults(
        &key,
        settings.encryption,
        options.repo_opts().password.as_bytes(),
    );

    // Figure out which type of repository they want, and create it
    match options.repo_opts().repository_type {
        RepositoryType::MultiFile => {
            // Create the directory
            create_dir_all(&options.repo_opts().repo)?;
            // Open the repository and set the key
            let mut mf = MultiFile::open_defaults(&options.repo_opts().repo, Some(settings), &key)
                .await
                .with_context(|| "Unable to create MultiFile directory.")?;
            mf.write_key(&encrypted_key)
                .await
                .with_context(|| "Failed to write key to new repository.")?;
            mf.close().await;
            Ok(())
        }
        RepositoryType::FlatFile => {
            // Open the repository setting the key
            let mut ff = FlatFile::new(
                &options.repo_opts().repo,
                Some(settings),
                Some(encrypted_key),
            )
            .with_context(|| "Unable to create flatfile.")?;
            ff.close().await;
            Ok(())
        }
    }
}
