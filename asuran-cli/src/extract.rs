use crate::cli::{GlobOpt, Opt};

use asuran::manifest::driver::*;
use asuran::manifest::target::*;
use asuran::manifest::*;
use asuran::repository::*;

use anyhow::Result;
use globset::{Glob, GlobSetBuilder};

use std::path::PathBuf;

/// Drives a repository and extracts the files from the user provided archive to
/// the user provided location
pub async fn extract(
    options: Opt,
    target: PathBuf,
    archive_name: String,
    glob_opts: GlobOpt,
    preview: bool,
) -> Result<()> {
    // Open the repository
    let (backend, key) = options.open_repo_backend().await?;
    let chunk_settings = options.get_chunk_settings();
    let mut repo = Repository::with(backend, chunk_settings, key, options.pipeline_tasks());
    // load the manifest
    let mut manifest = Manifest::load(&repo);
    // Load the list of archives
    let mut archives: Vec<ActiveArchive> = Vec::new();
    for stored_archive in manifest.archives().await {
        let archive = stored_archive.load(&mut repo).await?;
        archives.push(archive);
    }

    // Idenitify matching archives, and use the first one that matches the
    // string the user has provided us (on either its index in the list, or its
    // name)
    let mut matching_archives: Vec<ActiveArchive> = Vec::new();
    for (index, archive) in archives.into_iter().enumerate() {
        if index.to_string() == archive_name || archive.name() == archive_name {
            matching_archives.push(archive);
        }
    }

    // TODO (#36): Prompt the user when there are multiple matching archives
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
        // Build the includes glob
        let includes = if let Some(include_vec) = glob_opts.include {
            let mut builder = GlobSetBuilder::new();
            for include_string in include_vec {
                builder.add(Glob::new(&include_string)?);
            }
            Some(builder.build()?)
        } else {
            None
        };
        // Build the excludes glob
        let excludes = if let Some(exclude_vec) = glob_opts.exclude {
            let mut builder = GlobSetBuilder::new();
            for exclude_string in exclude_vec {
                builder.add(Glob::new(&exclude_string)?);
            }
            Some(builder.build()?)
        } else {
            None
        };
        // Load listing and setup target
        let listing = archive.listing().await;
        let f_target = FileSystemTarget::load_listing(target.to_str().unwrap(), listing).await;
        let paths = f_target
            .restore_listing()
            .await
            .into_iter()
            .filter(|x| includes.as_ref().map_or(true, |y| y.is_match(&x.path)))
            .filter(|x| excludes.as_ref().map_or(true, |y| !y.is_match(&x.path)));
        for node in paths {
            if !options.quiet {
                println!("Restoring file: {}", node.path);
            }
            // TODO (#36): properly utilize tasks here
            if !preview {
                f_target.retrieve_object(&mut repo, &archive, node).await?;
            }
        }
    }
    repo.close().await;
    Ok(())
}
