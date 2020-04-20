use crate::cli::*;

use asuran::manifest::*;
use asuran::prelude::*;

use anyhow::{anyhow, Result};
use globset::{Glob, GlobSetBuilder};

/// Lists the contents of a particular archive.
pub async fn contents(options: Opt, archive_name: String, glob_opts: GlobOpt) -> Result<()> {
    // First, open a connection to the repository
    let (backend, key) = options.open_repo_backend().await?;
    let chunk_settings = options.get_chunk_settings();
    let mut repo = Repository::with(backend, chunk_settings, key);
    // Load the manifest
    let mut manifest = Manifest::load(&repo);
    // Attempt to find a matching archive from the repository
    let mut matching_archive = None;
    for (index, stored_archive) in manifest.archives().await.into_iter().enumerate() {
        let archive = stored_archive.load(&mut repo).await?;
        if index.to_string() == archive_name || archive.name() == archive_name {
            matching_archive = Some(archive);
            break;
        }
    }

    match matching_archive {
        Some(archive) => {
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
            // Load the listing
            let listing = archive.listing().await;
            // Turn the listing into a set of paths, and filter it
            let listing = listing
                .into_iter()
                .map(|x| x.path)
                .filter(|x| includes.as_ref().map_or(true, |y| y.is_match(x)))
                .filter(|x| excludes.as_ref().map_or(true, |y| !y.is_match(x)));

            for path in listing {
                println!("{}", path);
            }

            Ok(())
        }
        _ => Err(anyhow!(
            "Provided archive name, {}, does not match any archives in the repository.",
            archive_name
        )),
    }
}
