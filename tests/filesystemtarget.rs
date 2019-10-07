use libasuran::chunker::*;
use libasuran::manifest::target::filesystem::*;
use libasuran::manifest::target::*;
use libasuran::manifest::*;
use libasuran::repository::*;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

mod common;

#[test]
fn backup_restore_no_empty_dirs() {
    let input_dir = fs::canonicalize("tests/inputdata/scodev1/").unwrap();
    let output_tempdir = tempdir().unwrap();
    let output_dir = output_tempdir.path();

    let repo_root = tempdir().unwrap();
    let repo_root_path = repo_root.path().to_str().unwrap();
    let key = Key::random(32);
    let mut repo = common::get_repo_bare(repo_root_path, key);
    let chunker = Chunker::new(6, 8, 0);

    let mut archive = Archive::new("test");

    let input_target = FileSystemTarget::new(input_dir.to_str().unwrap());
    let paths = input_target.backup_paths();
    for path in paths {
        println!("Backing up: {}", path);
        if fs::metadata(input_dir.join(Path::new(&path)))
            .unwrap()
            .is_file()
        {
            println!("Backing up {}", &path);
            let mut map = input_target.backup_object(&path);
            let object = map.remove("").unwrap();
            let mut ranges = object.ranges();
            // File is known to be dense, should only contain zero or one range
            assert!(ranges.len() == 1 || ranges.is_empty());
            if ranges.len() == 1 {
                archive.put_object(&chunker, &mut repo, &path, &mut ranges[0].object);
            }
        }
    }

    let listing = input_target.backup_listing();

    let mut manifest = Manifest::empty_manifest(common::get_bare_settings());
    manifest.commit_archive(&mut repo, archive);

    let manifest = Manifest::load(&repo);
    let stored_archive = &manifest.archives()[0];
    let archive = stored_archive.load(&repo).unwrap();

    let mut output_target =
        FileSystemTarget::load_listing(&listing).expect("Unable to reload listing");
    output_target.set_root_directory(&output_dir.to_str().unwrap());
    println!("Restoring to: {}", output_dir.to_str().unwrap());
    let paths = output_target.restore_listing();
    for path in paths {
        println!("Restoring: {}", path);
        let mut map = output_target.restore_object(&path);
        let object = map.remove("").unwrap();
        let mut ranges = object.ranges();
        // File is known to be flat, due to nature of test
        // Should only contain one range
        assert_eq!(1, ranges.len());
        archive.get_object(&repo, &path, &mut ranges[0].object);
    }

    assert!(!dir_diff::is_different(&input_dir, &output_dir).unwrap())
}
