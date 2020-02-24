use crate::cli::Opt;

use asuran::manifest::*;
use asuran::repository::*;

use anyhow::Result;
use prettytable::{cell, row, Table};

pub async fn list(options: Opt) -> Result<()> {
    // Open the repository
    let (backend, key) = options.open_repo_backend().await?;
    let chunk_settings = options.get_chunk_settings();
    let mut repo = Repository::with(backend, chunk_settings, key);
    // load the manifest
    let mut manifest = Manifest::load(&repo);
    // Get the list of archives and extract them from the repository
    let mut archives: Vec<ActiveArchive> = Vec::new();
    for stored_archive in manifest.archives().await {
        let archive = stored_archive.load(&mut repo).await?;
        archives.push(archive);
    }
    // Print out basic archive stats
    println!("Number of archives in repository: {}", archives.len());
    println!(
        "Repository last modified: {}",
        manifest.timestamp().await?.to_rfc2822()
    );
    // Iterate through the list of archives, and print them out in a nice table
    // TODO: sort by timestamp ascending
    // TODO: implement pagination
    let mut table = Table::new();
    table.add_row(row!["Index", "Name", "Creation Time"]);
    for (index, archive) in archives.into_iter().enumerate() {
        table.add_row(row![
            index,
            archive.name(),
            &archive.timestamp().to_rfc2822()
        ]);
    }
    table.printstd();
    repo.close().await;
    Ok(())
}
