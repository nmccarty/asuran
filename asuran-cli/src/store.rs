use crate::cli::Opt;

use asuran::chunker::*;
use asuran::manifest::driver::*;
use asuran::manifest::target::*;
use asuran::manifest::*;
use asuran::repository::*;

use anyhow::Result;
use chrono::prelude::*;
use futures::future::select_all;
use tokio::task;

use std::path::PathBuf;

/// Creates a new archive in a repository and inserts the files from the user
/// provided location
pub async fn store(options: Opt, target: PathBuf, name: Option<String>) -> Result<()> {
    // Open the repository
    let (backend, key) = options.open_repo_backend().await?;
    let chunk_settings = options.get_chunk_settings();
    let mut repo = Repository::with(backend, chunk_settings, key, options.pipeline_tasks());
    // Make sure we have a name for the archive, defaulting to the current
    // date/time if the user did not provide us one
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
    // Here, we maintain a vector of JoinHandles for the tasks we are spawning.
    // Whenever the vector is larger in size than max_queue_len, we use select
    // all to drain the first future from the queue to complete before
    // continuning. This ensures that we always only have to wait the minimum
    // ammount of time possible before continuning.
    //
    // TODO (#44): The job of managing the futures here really needs to be moved
    // into the `asuran` crate, with methods attached to BackupDriver for
    // managing this automatically. Both to improve ergonomics, as well as
    // reducing unnessicary clones.
    //
    // TODO: Either adapt max_queue_len based on the number and size of files,
    // or allow the user to set it. Higher numbers do better with lots of small
    // files, and smaller numbers do better with a small number of large files.
    let max_queue_len = 30;
    let mut task_queue = Vec::new();
    for node in paths {
        // Create clones of the values our task will need
        //
        // Spawining these tasks should really be backup_target's job, but
        // another alternative would be to elect to leak a refrence to these
        // values
        let mut repo = repo.clone();
        let archive = archive.clone();
        let backup_target = backup_target.clone();
        // Spawn a task and ask the target to store an object
        task_queue.push(task::spawn(async move {
            (
                node.clone(),
                backup_target
                    .store_object(&mut repo, chunker.clone(), &archive, node)
                    .await,
            )
        }));
        // Perform queue draining if we are over full.
        if task_queue.len() > max_queue_len {
            let (result, _, new_queue) = select_all(task_queue).await;
            let (node, x) = result?;
            x?;
            if !options.quiet {
                println!("Stored File: {}", node.path);
            }
            task_queue = new_queue;
        }
    }
    // Drain any remaining futures in the queue
    for future in task_queue {
        let (node, x) = future.await.unwrap();
        x?;
        println!("Stored File: {}", node.path);
    }
    // Add the backup listing to the archive
    let listing = backup_target.backup_listing().await;
    archive.set_listing(listing).await;
    // Commit the backup
    manifest.commit_archive(&mut repo, archive).await?;
    repo.close().await;
    Ok(())
}
