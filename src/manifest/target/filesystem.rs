pub mod metadata;

pub use metadata::*;

use super::*;
use crate::manifest::driver::*;
use async_std::sync::Mutex;
use async_trait::async_trait;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, metadata, File};
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;

#[derive(Clone)]
pub struct FileSystemTarget {
    root_directory: String,
    listing: Arc<Mutex<Vec<String>>>,
}

impl FileSystemTarget {
    pub fn new(root_directory: &str) -> FileSystemTarget {
        FileSystemTarget {
            root_directory: root_directory.to_string(),
            listing: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn set_root_directory(&mut self, new_root: &str) {
        self.root_directory = new_root.to_string();
    }
}

#[async_trait]
impl BackupTarget<File> for FileSystemTarget {
    async fn backup_paths(&self) -> Vec<String> {
        let mut output = Vec::new();
        for entry in WalkDir::new(&self.root_directory)
            .into_iter()
            .filter_map(Result::ok)
            .skip(1)
        {
            let rel_path = entry.path().strip_prefix(&self.root_directory).unwrap();
            output.push(rel_path.to_str().unwrap().to_string());
        }

        output
    }

    async fn backup_object(&self, path: &str) -> HashMap<String, BackupObject<File>> {
        let mut output = HashMap::new();
        // Get the actual path on the filesystem this refers to
        let root_path = Path::new(&self.root_directory);
        let rel_path = Path::new(path);
        let path = root_path.join(rel_path);
        // provide the actual data
        //
        // todo: add support for sparse files

        // Get the size of the file
        let meta = metadata(path.clone()).expect("Unable to read file metatdata");
        let mut file_object = BackupObject::new(meta.len());
        // An empty file has no extents
        if meta.len() > 0 {
            file_object.direct_add_range(
                0,
                meta.len() - 1,
                File::open(path).expect("Unable to open file"),
            );
        }
        output.insert("".to_string(), file_object);
        self.listing
            .lock()
            .await
            .push(rel_path.to_str().unwrap().to_string());
        output
    }

    async fn backup_listing(&self) -> Vec<u8> {
        let mut buff = Vec::<u8>::new();
        let listing = self.listing.lock().await;
        let listing = Vec::clone(&listing);
        listing.serialize(&mut Serializer::new(&mut buff)).unwrap();

        buff
    }
}

#[async_trait]
impl RestoreTarget<File> for FileSystemTarget {
    async fn load_listing(listing: &[u8]) -> Option<FileSystemTarget> {
        let mut de = Deserializer::new(listing);
        let listing: Vec<String> = Deserialize::deserialize(&mut de).ok()?;
        Some(FileSystemTarget {
            root_directory: "".to_string(),
            listing: Arc::new(Mutex::new(listing)),
        })
    }

    async fn restore_object(&self, path: &str) -> HashMap<String, RestoreObject<File>> {
        let mut output = HashMap::new();
        // Get the actual path on the filesystem this refers to
        let root_path = Path::new(&self.root_directory);
        let rel_path = Path::new(path);
        let path = root_path.join(rel_path);
        // Create the directory if it does not exist
        let parent = path.parent().unwrap();
        create_dir_all(parent).unwrap();

        // Return a writer to the file
        // TODO: Support for sparse file
        // TODO: Filesize support
        let mut file_object = RestoreObject::new(0);
        // FIXME: Currently does not have filesize info
        // FIXME: Currently misbehaves and still returns a range for a zero sized file
        file_object.direct_add_range(
            0,
            0,
            File::create(path.clone()).expect("Unable to open file"),
        );
        output.insert("".to_string(), file_object);
        output
    }

    async fn restore_listing(&self) -> Vec<String> {
        self.listing.lock().await.clone()
    }
}

impl BackupDriver<File> for FileSystemTarget {}
impl RestoreDriver<File> for FileSystemTarget {}

#[cfg(test)]
mod tests {
    use super::*;
    use dir_diff;
    use futures::executor::block_on;
    use std::fs::{create_dir, File};
    use tempfile::{tempdir, TempDir};

    fn make_test_directory() -> TempDir {
        let root = tempdir().unwrap();
        let root_path = root.path();

        create_dir(root_path.join("A")).unwrap();
        create_dir(root_path.join("B")).unwrap();
        create_dir(root_path.join("B").join("C")).unwrap();

        File::create(root_path.join("1")).unwrap();
        File::create(root_path.join("2")).unwrap();
        File::create(root_path.join("3")).unwrap();
        File::create(root_path.join("A").join("4")).unwrap();
        File::create(root_path.join("B").join("5")).unwrap();
        File::create(root_path.join("B").join("C").join("6")).unwrap();

        root
    }

    #[test]
    fn backup_restore_structure() {
        block_on(async {
            let input_dir = make_test_directory();
            let root_path = input_dir.path().to_owned();

            let input_target = FileSystemTarget::new(&root_path.display().to_string());

            for item in WalkDir::new(&root_path)
                .into_iter()
                .map(|e| e.unwrap())
                .filter(|e| e.file_type().is_file())
            {
                let rel_path = item
                    .path()
                    .strip_prefix(&root_path)
                    .unwrap()
                    .display()
                    .to_string();
                println!("Backing up: {}", &rel_path);
                input_target.backup_object(&rel_path).await;
            }

            let listing = input_target.backup_listing().await;

            let output_dir = tempdir().unwrap();

            let mut output_target = FileSystemTarget::load_listing(&listing)
                .await
                .expect("Failed to unwrap packed listing");
            output_target.set_root_directory(&output_dir.path().display().to_string());

            let output_listing = output_target.restore_listing().await;
            for entry in output_listing {
                println!();
                println!("Restore listing:");
                println!(" - {}", &entry);
                output_target.restore_object(&entry).await;
            }

            let _input_path = input_dir.path().display().to_string();
            let _output_path = output_dir.path().display().to_string();

            assert!(!dir_diff::is_different(&input_dir.path(), &output_dir.path()).unwrap());
        });
    }
}
