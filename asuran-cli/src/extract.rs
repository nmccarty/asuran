use crate::cli::Opt;

use asuran::manifest::driver::*;
use asuran::manifest::target::*;
use asuran::manifest::*;
use asuran::repository::*;

use anyhow::Result;
use std::path::PathBuf;

pub async fn extract(options: Opt, target: PathBuf, archive_name: String) -> Result<()> {
    // Open the repository
    let (backend, key) = options.open_repo_backend().await?;
    let chunk_settings = options.get_chunk_settings();
    let mut repo = Repository::with(backend, chunk_settings, key);
    // load the manifest
    let mut manifest = Manifest::load(&repo);
    // Load the list of archives
    let mut archives: Vec<ActiveArchive> = Vec::new();
    for stored_archive in manifest.archives().await {
        let archive = stored_archive.load(&mut repo).await?;
        archives.push(archive);
    }

    let mut matching_archives: Vec<ActiveArchive> = Vec::new();
    for (index, archive) in archives.into_iter().enumerate() {
        if index.to_string() == archive_name || archive.name() == archive_name {
            matching_archives.push(archive);
        }
    }

    // TODO: Prompt the user when there are multiple matching archives
    // For now, just use the first match
    if matching_archives.is_empty() {
        println!("No matching archives found.");
    } else {
        let archive = &matching_archives[0];
        println!(
            "Using archive {} taken at {}",
            archive.name(),
            archive.timestamp().to_rfc2822()
        );
        // Load listing and setup target
        let mut listing = Vec::<u8>::new();
        archive
            .namespace_append("meta")
            .get_object(&mut repo, "listing", &mut listing)
            .await?;
        let mut f_target = FileSystemTarget::load_listing(&listing).await.unwrap();
        f_target.set_root_directory(target.to_str().unwrap());
        let paths = f_target.restore_listing().await;
        for path in paths {
            println!("Restoring file: {}", path);
            // TODO: properly utilize tasks here
            f_target.retrieve_object(&mut repo, &archive, &path).await?;
        }
    }
    repo.close().await;
    Ok(())
}
