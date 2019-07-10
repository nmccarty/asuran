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
    use std::fs::{create_dir, File};
    use tempfile::{tempdir, TempDir};

    /// Create a testing directory structure that looks like so:
    ///
    /// ```
    /// root:
    ///     1
    ///     2
    ///     3
    ///     A:
    ///         4
    ///     B:
    ///         5
    ///         C:
    ///             6
    ///     
    /// ```
    fn mk_tmp_dir() -> TempDir {
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
        File::create(root_path.join("C").join("6")).unwrap();

        root
    }
}
