use asuran::chunker::*;
use asuran::manifest::driver::*;
use asuran::manifest::target::*;
use asuran::manifest::*;
use asuran::repository::backend::multifile::*;
use asuran::repository::*;

use anyhow::Result;
use chrono::prelude::*;
use clap::{load_yaml, App, AppSettings, ArgMatches};
use prettytable::{cell, row, Table};
use rpassword::read_password_from_tty;
use std::boxed::Box;
use std::fs;
use std::fs::create_dir_all;
use std::io::Cursor;
use std::path::Path;

mod util;

use util::*;

fn start_app_get_matches() -> ArgMatches<'static> {
    let version = format!(
        "{}-{} {}",
        env!("VERGEN_SEMVER"),
        env!("VERGEN_SHA_SHORT"),
        env!("VERGEN_BUILD_DATE")
    );
    // We are going to make a hacky decision to leak the yaml
    //
    // This is because our app runs on a threadpool executor, and it not being 'static causes major
    // lifetime headaches
    //
    // This is substantially less than ideal, but it leaks a minimal ammount of memory, and only a
    // single time.
    let yaml = {
        let tmp_yaml = load_yaml!("cli.yml").clone();
        let tmp = Box::new(tmp_yaml);
        Box::leak(tmp)
    };

    App::from_yaml(yaml)
        .version(version.as_str())
        .setting(AppSettings::ArgRequiredElseHelp)
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .get_matches()
}

#[tokio::main]
async fn main() {
    let matches = start_app_get_matches();

    let settings = ChunkSettings {
        compression: Compression::ZStd { level: 1 },
        encryption: Encryption::new_aes256ctr(),
        hmac: HMAC::Blake3,
    };

    // Determine if a password was given, if not, prompt for it
    // TODO: Also check an enviroment variable
    let password = if let Some(password) = matches.value_of("password") {
        password.to_string()
    } else {
        read_password_from_tty(Some("Repository Password: ")).unwrap()
    };

    let repo = matches.value_of("REPO").unwrap().to_string();

    match matches.subcommand() {
        ("new", _) => new(repo, password, settings).await,
        ("list", _) => list(repo, &password).await,
        ("store", Some(m)) => store(repo, m.clone(), &password).await,
        ("retrive", Some(m)) => retrive(repo, m.clone(), &password).await,
        _ => unreachable!(),
    }
    .unwrap();
}

/// Creates a new repository in the given target directory
async fn new(repo_path: String, password: String, settings: ChunkSettings) -> Result<()> {
    // TODO: Add support for selecting default parameters
    // Create directory if it does not exist
    create_dir_all(&repo_path).expect("Unable to create repository directory.");
    // Select encryption
    let encryption = Encryption::new_aes256ctr();
    // Create a new key
    let key = Key::random(encryption.key_length());
    // Setup backend
    let backend = MultiFile::open_defaults(repo_path, Some(settings), &key)?;
    // Encrypt key and store to backend
    let enc_key = EncryptedKey::encrypt_defaults(&key, encryption, password.as_bytes());
    backend
        .write_key(&enc_key)
        .await
        .expect("Unable to write key to backend.");
    // Setup repository and write an empty manifest, then commit
    let repo = Repository::new(
        backend,
        settings.compression,
        settings.hmac,
        settings.encryption,
        key,
    );
    let mut manifest = Manifest::load(&repo);
    manifest.set_chunk_settings(repo.chunk_settings()).await;

    repo.commit_index().await;
    repo.close().await;
    Ok(())
}

/// Lists the archives in a repository
async fn list(repo_path: String, password: &str) -> Result<()> {
    // Open the repository and exract the manfest
    let mut repo = open_repo_filesystem(repo_path, password.as_bytes(), None).await?;
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
        manifest.timestamp().await.to_rfc2822()
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

/// Creates an archive in the repository
async fn store(repo_path: String, m: ArgMatches<'static>, password: &str) -> Result<()> {
    // Open repo and manifest
    let mut repo = open_repo_filesystem(repo_path, password.as_bytes(), None).await?;
    let mut manifest = Manifest::load(&repo);
    // Determine the name of the archive
    let name = if let Some(name) = m.value_of("name") {
        name.to_string()
    } else {
        Local::now()
            .with_timezone(Local::now().offset())
            .to_rfc3339()
    };
    // Create archive
    let archive = ActiveArchive::new(&name);
    // load in target
    let target_path = m.value_of("TARGET").unwrap();
    // Setup the backup target
    // Default chunker has a 128 byte window and is aming for 512kiB chunks
    let chunker = FastCDC::default();
    let absoulte_path = fs::canonicalize(target_path).expect("Failed to expand target path");
    let target = FileSystemTarget::new(absoulte_path.to_str().unwrap());
    let paths = target.backup_paths().await;
    // Run the backup
    for path in paths {
        if fs::metadata(absoulte_path.join(Path::new(&path)))
            .unwrap()
            .is_file()
        {
            target
                .store_object(&mut repo, chunker.clone(), &archive, path.clone())
                .await?;
            println!("{}", &path);
        }
    }
    // Add the backup listing to the archive
    let listing = Cursor::new(target.backup_listing().await);
    archive
        .namespace_append("meta")
        .put_object(&chunker, &mut repo, "listing", listing)
        .await?;
    // Commit the backup
    manifest.commit_archive(&mut repo, archive).await;
    repo.close().await;
    Ok(())
}

/// Restores an archive an archive from the repository
async fn retrive(repo_path: String, m: ArgMatches<'_>, password: &str) -> Result<()> {
    // Open repo and manifest
    let mut repo = open_repo_filesystem(repo_path, password.as_bytes(), None).await?;
    let mut manifest = Manifest::load(&repo);
    // Get the list of archives and extract them from the repository
    let mut archives: Vec<ActiveArchive> = Vec::new();
    for stored_archive in manifest.archives().await {
        let archive = stored_archive.load(&mut repo).await?;
        archives.push(archive);
    }
    // get name from arguments
    let name = m.value_of("ARCHIVE").unwrap();
    // Get the restore target from arguments
    let target_path = m.value_of("TARGET").unwrap();
    // Find matching archives
    let mut matching_archives: Vec<ActiveArchive> = Vec::new();
    for (index, archive) in archives.into_iter().enumerate() {
        if index.to_string() == name || archive.name() == name {
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
        let mut target = FileSystemTarget::load_listing(&listing).await.unwrap();
        target.set_root_directory(target_path);
        let paths = target.restore_listing().await;
        for path in paths {
            println!("{}", path);
            target.retrieve_object(&mut repo, &archive, &path).await?;
        }
    }
    repo.close().await;
    Ok(())
}
