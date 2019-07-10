pub mod metadata;

pub use metadata::*;

use super::*;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::path::Path;
use std::sync::{Arc, Mutex};
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

impl BackupTarget for FileSystemTarget {
    fn backup_paths(&self) -> Vec<String> {
        let mut output = Vec::new();
        for entry in WalkDir::new(&self.root_directory)
            .into_iter()
            .filter_map(Result::ok)
        {
            let rel_path = entry.path().strip_prefix(&self.root_directory).unwrap();
            output.push(rel_path.to_str().unwrap().to_string());
        }

        output
    }

    fn backup_object(&self, path: &str) -> HashMap<String, BackupObject> {
        let mut output = HashMap::new();
        // Get the actual path on the filesystem this refers to
        let root_path = Path::new(&self.root_directory);
        let rel_path = Path::new(path);
        let path = root_path.join(rel_path);
        // provide the actual data
        //
        // todo: add support for sparse files
        output.insert(
            "".to_string(),
            BackupObject::Dense {
                object: Box::new(File::open(path.clone()).expect("Unable to open file")),
            },
        );
        self.listing
            .lock()
            .unwrap()
            .push(path.to_str().unwrap().to_string());
        output
    }

    fn backup_listing(&self) -> Vec<u8> {
        let mut buff = Vec::<u8>::new();
        let listing = self.listing.lock().unwrap();
        let listing = Vec::clone(&listing);
        listing.serialize(&mut Serializer::new(&mut buff)).unwrap();

        buff
    }
}

impl RestoreTarget for FileSystemTarget {
    fn load_listing(listing: &[u8]) -> Option<FileSystemTarget> {
        let mut de = Deserializer::new(listing);
        let listing: Vec<String> = Deserialize::deserialize(&mut de).ok()?;
        Some(FileSystemTarget {
            root_directory: "".to_string(),
            listing: Arc::new(Mutex::new(listing)),
        })
    }

    fn restore_object(&self, path: &str) -> HashMap<String, RestoreObject> {
        let mut output = HashMap::new();
        // Get the actual path on the filesystem this refers to
        let root_path = Path::new(&self.root_directory);
        let rel_path = Path::new(path);
        let path = root_path.join(rel_path);
        // Create the directory if it does not exist
        let parent = path.parent().unwrap();
        create_dir_all(parent).unwrap();

        // Return a writer to the file
        output.insert(
            "".to_string(),
            RestoreObject::Dense {
                object: Box::new(File::create(path.clone()).expect("Unable to open file")),
            },
        );
        self.listing
            .lock()
            .unwrap()
            .push(path.to_str().unwrap().to_string());
        output
    }

    fn restore_listing(&self) -> Vec<String> {
        self.listing.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dir_diff;
    use std::fs::{create_dir, File};
    use std::process::Command;
    use std::str;
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
        let input_dir = make_test_directory();
        let root_path = input_dir.path().to_owned();

        let input_target = FileSystemTarget::new(&root_path.display().to_string());

        for item in WalkDir::new(&root_path)
            .into_iter()
            .filter_entry(|e| e.file_type().is_file())
        {
            let item = item.unwrap();
            let rel_path = item
                .path()
                .strip_prefix(&root_path)
                .unwrap()
                .display()
                .to_string();
            input_target.backup_object(&rel_path);
        }

        let listing = input_target.backup_listing();

        let output_dir = tempdir().unwrap();

        let mut output_target =
            FileSystemTarget::load_listing(&listing).expect("Failed to unwrap packed listing");
        output_target.set_root_directory(&output_dir.path().display().to_string());

        let output_listing = output_target.restore_listing();
        for entry in output_listing {
            output_target.restore_object(&entry);
        }

        let input_path = input_dir.path().display().to_string();
        let output_path = output_dir.path().display().to_string();

        println!("Contents of input directory ({}):", input_path);
        let output = Command::new("/usr/bin/tree")
            .arg(input_path)
            .output()
            .unwrap();
        println!("{}", str::from_utf8(&output.stdout).unwrap());

        println!("Contents of output directory ({}):", output_path);
        let output = Command::new("/usr/bin/tree")
            .arg(output_path)
            .output()
            .unwrap();
        println!("{}", str::from_utf8(&output.stdout).unwrap());

        assert!(!dir_diff::is_different(&input_dir.path(), &output_dir.path()).unwrap());
    }

}
