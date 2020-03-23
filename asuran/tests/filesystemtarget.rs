use asuran::chunker::*;
use asuran::manifest::driver::*;
use asuran::manifest::target::filesystem::*;
use asuran::manifest::target::*;
use asuran::manifest::*;
use asuran::repository::*;
use std::fs;
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
    let mut repo = common::get_repo_bare(repo_root_path, key).await;
    let chunker = FastCDC::default();

    let archive = ActiveArchive::new("test");

    let input_target = FileSystemTarget::new(input_dir.to_str().unwrap());
    let paths = input_target.backup_paths().await;
    for node in paths {
        println!("Backing up: {}", node.path);
        input_target
            .store_object(&mut repo, chunker.clone(), &archive, node)
            .await
            .unwrap();
    }

    let listing = input_target.backup_listing().await;
    archive.set_listing(listing).await;

    let mut manifest = Manifest::load(&mut repo);
    manifest.commit_archive(&mut repo, archive).await.unwrap();
    repo.commit_index().await;

    let mut manifest = Manifest::load(&mut repo);
    let stored_archive = &manifest.archives().await[0];
    let archive = stored_archive.load(&mut repo).await.unwrap();

    let output_target =
        FileSystemTarget::load_listing(&output_dir.to_str().unwrap(), archive.listing().await)
            .await;
    println!("Restoring to: {}", output_dir.to_str().unwrap());
    let paths = output_target.restore_listing().await;
    for node in paths {
        println!("Restoring: {}", node.path);
        output_target
            .retrieve_object(&mut repo, &archive, node)
            .await
            .unwrap();
    }

    assert!(!dir_diff::is_different(&input_dir, &output_dir).unwrap());
    repo.close().await;
}

#[tokio::test(threaded_scheduler)]
async fn backup_restore_no_empty_dirs_flatfile() {
    let input_dir = fs::canonicalize("tests/inputdata/scodev1/").unwrap();
    let output_tempdir = tempdir().unwrap();
    let output_dir = output_tempdir.path();

    let tempdir = tempdir().unwrap();
    let path = tempdir.path().join("test.asuran");
    let password = b"A Very Strong Password";
    let key = Key::random(32);
    let enc_key = EncryptedKey::encrypt(&key, 512, 1, Encryption::new_aes256ctr(), password);
    // Since we are opening the repo for the first time, we provide the key here
    let mut repo = common::get_repo_flat(path.clone(), key.clone(), Some(enc_key));

    let chunker = FastCDC::default();

    let archive = ActiveArchive::new("test");

    let input_target = FileSystemTarget::new(input_dir.to_str().unwrap());
    let paths = input_target.backup_paths().await;
    for node in paths {
        println!("Backing up: {}", node.path);
        input_target
            .store_object(&mut repo, chunker.clone(), &archive, node)
            .await
            .unwrap();
    }

    let listing = input_target.backup_listing().await;
    archive.set_listing(listing).await;

    let mut manifest = Manifest::load(&mut repo);
    manifest.commit_archive(&mut repo, archive).await.unwrap();
    repo.commit_index().await;

    repo.close().await;

    let mut repo = common::get_repo_flat(path.clone(), key, None);

    let mut manifest = Manifest::load(&mut repo);
    let stored_archive = &manifest.archives().await[0];
    let archive = stored_archive.load(&mut repo).await.unwrap();

    let output_target =
        FileSystemTarget::load_listing(&output_dir.to_str().unwrap(), archive.listing().await)
            .await;
    println!("Restoring to: {}", output_dir.to_str().unwrap());
    let paths = output_target.restore_listing().await;
    for node in paths {
        println!("Restoring: {}", node.path);
        output_target
            .retrieve_object(&mut repo, &archive, node)
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

    let archive = ActiveArchive::new("test");

    let input_target = FileSystemTarget::new(input_dir.to_str().unwrap());
    let paths = input_target.backup_paths().await;
    for node in paths {
        println!("Backing up: {}", node.path);
        input_target
            .store_object(&mut repo, chunker.clone(), &archive, node)
            .await
            .unwrap();
    }

    let listing = input_target.backup_listing().await;
    archive.set_listing(listing).await;

    let mut manifest = Manifest::load(&mut repo);
    manifest.commit_archive(&mut repo, archive).await.unwrap();
    repo.commit_index().await;

    let mut manifest = Manifest::load(&mut repo);
    let stored_archive = &manifest.archives().await[0];
    let archive = stored_archive.load(&mut repo).await.unwrap();

    let output_target =
        FileSystemTarget::load_listing(&output_dir.to_str().unwrap(), archive.listing().await)
            .await;
    println!("Restoring to: {}", output_dir.to_str().unwrap());
    let paths = output_target.restore_listing().await;
    for node in paths {
        println!("Restoring: {}", node.path);
        output_target
            .retrieve_object(&mut repo, &archive, node)
            .await
            .unwrap();
    }

    assert!(!dir_diff::is_different(&input_dir, &output_dir).unwrap());
    repo.close().await;
}
