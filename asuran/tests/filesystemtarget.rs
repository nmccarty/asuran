use asuran::chunker::*;
use asuran::manifest::driver::*;
use asuran::manifest::target::filesystem::*;
use asuran::manifest::target::*;
use asuran::manifest::*;
use asuran::repository::*;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

mod common;

#[tokio::test(threaded_scheduler)]
async fn backup_restore_no_empty_dirs_filesystem() {
    let input_dir = fs::canonicalize("tests/inputdata/scodev1/").unwrap();
    let output_tempdir = tempdir().unwrap();
    let output_dir = output_tempdir.path();

    let repo_root = tempdir().unwrap();
    let repo_root_path = repo_root.path().to_str().unwrap();
    let key = Key::random(32);
    let mut repo = common::get_repo_bare(repo_root_path, key);
    let chunker = FastCDC::default();

    let archive = Archive::new("test");

    let input_target = FileSystemTarget::new(input_dir.to_str().unwrap());
    let paths = input_target.backup_paths().await;
    for path in paths {
        println!("Backing up: {}", path);
        if fs::metadata(input_dir.join(Path::new(&path)))
            .unwrap()
            .is_file()
        {
            println!("Backing up {}", &path);
            input_target
                .store_object(&mut repo, chunker.clone(), &archive, path.clone())
                .await
                .unwrap();
        }
    }

    let listing = input_target.backup_listing().await;

    let mut manifest = Manifest::load(&mut repo);
    manifest.commit_archive(&mut repo, archive).await;
    repo.commit_index().await;

    let mut manifest = Manifest::load(&mut repo);
    let stored_archive = &manifest.archives().await[0];
    let archive = stored_archive.load(&mut repo).await.unwrap();

    let mut output_target = FileSystemTarget::load_listing(&listing)
        .await
        .expect("Unable to reload listing");
    output_target.set_root_directory(&output_dir.to_str().unwrap());
    println!("Restoring to: {}", output_dir.to_str().unwrap());
    let paths = output_target.restore_listing().await;
    for path in paths {
        println!("Restoring: {}", path);
        output_target
            .retrieve_object(&mut repo, &archive, &path)
            .await
            .unwrap();
    }

    assert!(!dir_diff::is_different(&input_dir, &output_dir).unwrap());
    repo.close().await;
}

#[tokio::test(threaded_scheduler)]
async fn backup_restore_no_empty_dirs_mem() {
    let input_dir = fs::canonicalize("tests/inputdata/scodev1/").unwrap();
    let output_tempdir = tempdir().unwrap();
    let output_dir = output_tempdir.path();

    let key = Key::random(32);
    let mut repo = common::get_repo_mem(key);
    let chunker = FastCDC::default();

    let archive = Archive::new("test");

    let input_target = FileSystemTarget::new(input_dir.to_str().unwrap());
    let paths = input_target.backup_paths().await;
    for path in paths {
        println!("Backing up: {}", path);
        if fs::metadata(input_dir.join(Path::new(&path)))
            .unwrap()
            .is_file()
        {
            println!("Backing up {}", &path);
            input_target
                .store_object(&mut repo, chunker.clone(), &archive, path.clone())
                .await
                .unwrap();
        }
    }

    let listing = input_target.backup_listing().await;

    let mut manifest = Manifest::load(&mut repo);
    manifest.commit_archive(&mut repo, archive).await;

    let mut manifest = Manifest::load(&mut repo);
    let stored_archives = &manifest.archives().await;
    let stored_archive = &stored_archives[0];
    let archive = stored_archive.load(&mut repo).await.unwrap();
    println!("{:?}", archive);

    let mut output_target = FileSystemTarget::load_listing(&listing)
        .await
        .expect("Unable to reload listing");
    output_target.set_root_directory(&output_dir.to_str().unwrap());
    println!("Restoring to: {}", output_dir.to_str().unwrap());
    let paths = output_target.restore_listing().await;
    for path in paths {
        println!("Restoring: {}", path);
        output_target
            .retrieve_object(&mut repo, &archive, &path)
            .await
            .unwrap();
    }

    assert!(!dir_diff::is_different(&input_dir, &output_dir).unwrap())
}
