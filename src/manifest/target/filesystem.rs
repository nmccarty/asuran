pub mod metadata;

pub use metadata::*;

use super::*;
use rmp_serde::{Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::fs::File;
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
                object: Box::new(File::open(path).expect("Unable to open file")),
            },
        );
        output
    }

    fn backup_finalize(&self, path: &str) {
        unimplemented!();
    }

    fn backup_complete(&self) {
        unimplemented!();
    }

    fn backup_listing(&self) -> Vec<u8> {
        unimplemented!();
    }
}
