use crate::cli::Opt;

use asuran::chunker::*;
use asuran::manifest::driver::*;
use asuran::manifest::target::*;
use asuran::manifest::*;
use asuran::repository::*;

use anyhow::{Context, Result};
use chrono::prelude::*;
use futures::future::select_all;
use std::fs::metadata;
use std::io::Cursor;
use std::path::PathBuf;
use tokio::task;

pub async fn store(options: Opt, target: PathBuf, name: Option<String>) -> Result<()> {
    // Open the repository
    let (backend, key) = options.open_repo_backend().await?;
    let chunk_settings = options.get_chunk_settings();
    let mut repo = Repository::with(backend, chunk_settings, key);
    // Make sure we have a name for the archive, defaulting to the current date/time
    let name = name.unwrap_or_else(|| {
        Local::now()
            .with_timezone(Local::now().offset())
            .to_rfc2822()
    });
    // Load the manifest and create the archive
    let mut manifest = Manifest::load(&repo);
    let archive = ActiveArchive::new(&name);
    // TOOD: Allow chunker configuration
    let chunker = FastCDC::default();
    // Load the target
    let backup_target = FileSystemTarget::new(target.to_str().unwrap());
    // Run the backup
    let paths = backup_target.backup_paths().await;
    // Here we use a VecDeque of futures to keep track of the store_object futures we
    // have created and started tasks for.
    //
    // We will fill the queue up to `max_queue_len`, and once we hit that limit, we
    // will pop off the oldest task future and `await`ing it, but only after staring
    // the next task.
    //
    // TODO: The job of managing the futures here really needs to be moved into the `asuran` crate, with
    // methods attached to BackupDriver for managing this automatically. Both to improve ergonomics, as
    // well as reducing unnessicary clones.
    let max_queue_len = 10;
    let mut task_queue = Vec::new();
    for path in paths {
        if metadata(target.join(&path))
            .with_context(|| format!("Failed to load file {}", &path))?
            .is_file()
        {
            let mut repo = repo.clone();
            let archive = archive.clone();
            let backup_target = backup_target.clone();

            task_queue.push(task::spawn(async move {
                (
                    path.clone(),
                    backup_target
                        .store_object(&mut repo, chunker.clone(), &archive, path.clone())
                        .await,
                )
            }));

            if task_queue.len() > max_queue_len {
                let (result, _, new_queue) = select_all(task_queue).await;
                let (path, x) = result?;
                x?;
                println!("Stored File: {}", path);
                task_queue = new_queue;
            }
        }
    }
    // Drain any remaining futures in the queue
    for future in task_queue {
        let (path, x) = future.await.unwrap();
        x?;
        println!("Stored File: {}", path);
    }
    // Add the backup listing to the archive
    let listing = Cursor::new(backup_target.backup_listing().await);
    archive
        .namespace_append("meta")
        .put_object(&chunker, &mut repo, "listing", listing)
        .await?;
    // Commit the backup
    manifest.commit_archive(&mut repo, archive).await?;
    repo.close().await;
    Ok(())
}
