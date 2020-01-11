use futures::executor::block_on;
use libasuran::chunker::slicer::fastcdc::FastCDC;
use libasuran::chunker::*;
use libasuran::manifest::driver::*;
use libasuran::manifest::target::filesystem::*;
use libasuran::manifest::target::*;
use libasuran::manifest::*;
use libasuran::repository::*;
use std::fs;
use std::io::Empty;
use std::path::Path;
use tempfile::tempdir;

mod common;

// #[test]
// fn backup_restore_no_empty_dirs_filesystem() {
//     block_on(async {
//         let input_dir = fs::canonicalize("tests/inputdata/scodev1/").unwrap();
//         let output_tempdir = tempdir().unwrap();
//         let output_dir = output_tempdir.path();

//         let repo_root = tempdir().unwrap();
//         let repo_root_path = repo_root.path().to_str().unwrap();
//         let key = Key::random(32);
//         let mut repo = common::get_repo_bare(repo_root_path, key);
//         let slicer: FastCDC<Empty> = FastCDC::new_defaults();
//         let chunker = Chunker::new(slicer.copy_settings());

//         let archive = Archive::new("test");

//         let input_target = FileSystemTarget::new(input_dir.to_str().unwrap());
//         let paths = input_target.backup_paths();
//         for path in paths {
//             println!("Backing up: {}", path);
//             if fs::metadata(input_dir.join(Path::new(&path)))
//                 .unwrap()
//                 .is_file()
//             {
//                 println!("Backing up {}", &path);
//                 input_target
//                     .store_object(&mut repo, chunker.clone(), &archive, path.clone())
//                     .await
//                     .unwrap();
//             }
//         }

//         let listing = input_target.backup_listing();

//         let mut manifest = Manifest::load(&mut repo);
//         manifest.commit_archive(&mut repo, archive).await;
//         repo.commit_index();

//         let manifest = Manifest::load(&mut repo);
//         let stored_archive = &manifest.archives()[0];
//         let archive = stored_archive.load(&mut repo).await.unwrap();

//         let mut output_target =
//             FileSystemTarget::load_listing(&listing).expect("Unable to reload listing");
//         output_target.set_root_directory(&output_dir.to_str().unwrap());
//         println!("Restoring to: {}", output_dir.to_str().unwrap());
//         let paths = output_target.restore_listing();
//         for path in paths {
//             println!("Restoring: {}", path);
//             output_target
//                 .retrieve_object(&mut repo, &archive, &path)
//                 .await
//                 .unwrap();
//         }

//         assert!(!dir_diff::is_different(&input_dir, &output_dir).unwrap())
//     });
// }

#[test]
fn backup_restore_no_empty_dirs_mem() {
    block_on(async {
        let input_dir = fs::canonicalize("tests/inputdata/scodev1/").unwrap();
        let output_tempdir = tempdir().unwrap();
        let output_dir = output_tempdir.path();

        let key = Key::random(32);
        let mut repo = common::get_repo_mem(key);
        let slicer: FastCDC<Empty> = FastCDC::new_defaults();
        let chunker = Chunker::new(slicer.copy_settings());

        let archive = Archive::new("test");

        let input_target = FileSystemTarget::new(input_dir.to_str().unwrap());
        let paths = input_target.backup_paths();
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

        let listing = input_target.backup_listing();

        let mut manifest = Manifest::load(&mut repo);
        manifest.commit_archive(&mut repo, archive).await;

        let mut manifest = Manifest::load(&mut repo);
        let stored_archives = &manifest.archives().await;
        let stored_archive = &stored_archives[0];
        let archive = stored_archive.load(&mut repo).await.unwrap();
        println!("{:?}", archive);

        let mut output_target =
            FileSystemTarget::load_listing(&listing).expect("Unable to reload listing");
        output_target.set_root_directory(&output_dir.to_str().unwrap());
        println!("Restoring to: {}", output_dir.to_str().unwrap());
        let paths = output_target.restore_listing();
        for path in paths {
            println!("Restoring: {}", path);
            output_target
                .retrieve_object(&mut repo, &archive, &path)
                .await
                .unwrap();
        }

        assert!(!dir_diff::is_different(&input_dir, &output_dir).unwrap())
    });
}
