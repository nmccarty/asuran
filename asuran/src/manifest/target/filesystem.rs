#![allow(unused_variables)]
use super::*;
use crate::manifest::archive::Extent;
use crate::manifest::driver::*;

use async_trait::async_trait;
use std::fs::{create_dir_all, File};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task;
use walkdir::WalkDir;

#[derive(Clone)]
/// A type that handles the complexities of dealing with a file system for you.
pub struct FileSystemTarget {
    root_directory: String,
    listing: Arc<RwLock<Listing>>,
}

impl FileSystemTarget {
    /// Creates a new FileSystemTarget with the given path as its top level directory.
    ///
    /// The FileSystemTarget will consider all paths below this directory for backup.
    pub fn new(root_directory: &str) -> FileSystemTarget {
        FileSystemTarget {
            root_directory: root_directory.to_string(),
            listing: Arc::new(RwLock::new(Listing::default())),
        }
    }

    pub fn set_root_directory(&mut self, new_root: &str) {
        self.root_directory = new_root.to_string();
    }
}

#[async_trait]
impl BackupTarget<File> for FileSystemTarget {
    async fn backup_paths(&self) -> Listing {
        let mut listing = Listing::default();
        for entry in WalkDir::new(&self.root_directory)
            .into_iter()
            .filter_map(Result::ok)
            .skip(1)
        {
            let rel_path = entry
                .path()
                .strip_prefix(&self.root_directory)
                .expect("Failed getting realtive path in file system target")
                .to_owned();
            let parent_path = rel_path
                .parent()
                .expect("Failed getting parent path in filesystem target");
            let metadata = {
                let path = entry.path().to_owned();
                task::spawn_blocking(move || {
                    path.metadata().expect("Failed getting file metatdata")
                })
                .await
                .expect("Failed to join blocking task")
            };
            // FIXME: Making an assuming that the object is either a file or a directory
            let node_type = if metadata.is_file() {
                NodeType::File
            } else {
                NodeType::Directory {
                    children: Vec::new(),
                }
            };

            let path = rel_path
                .to_str()
                .expect("Path contained non-utf8")
                .to_string();

            let extents = if metadata.is_file() && metadata.len() > 0 {
                Some(vec![Extent {
                    start: 0,
                    end: metadata.len() - 1,
                }])
            } else {
                None
            };

            let node = Node {
                path,
                total_length: metadata.len(),
                total_size: metadata.len(),
                extents,
                node_type,
            };

            listing.add_child(parent_path.to_str().expect("Path contained non-utf8"), node);
        }
        listing
    }
    async fn backup_object(&self, node: Node) -> HashMap<String, BackupObject<File>> {
        let mut output = HashMap::new();
        // FIXME: Store directory metatdata
        if node.is_file() {
            // Get the actual path on the filesystem this referes to
            let root_path = Path::new(&self.root_directory);
            let path = root_path.join(&node.path);
            // Construct the file_object based on the information in the node
            let mut file_object = BackupObject::new(node.total_length);
            // add each extent from the node to the object
            if let Some(extents) = node.extents.as_ref() {
                for extent in extents {
                    let file = {
                        let path = path.clone();
                        task::spawn_blocking(move || {
                            File::open(&path).expect("Unable to open file")
                        })
                        .await
                        .expect("unable to join spawned task")
                    };
                    file_object.direct_add_range(extent.start, extent.end, file);
                }
            }
            output.insert(String::new(), file_object);
        }
        let path = node.path.clone();
        let parent_path = Path::new(&path)
            .parent()
            .expect("Unable to get parent path")
            .to_str()
            .expect("Invalid utf-8 in path");
        self.listing.write().await.add_child(parent_path, node);
        output
    }
    async fn backup_listing(&self) -> Listing {
        self.listing.read().await.clone()
    }
}

#[async_trait]
impl RestoreTarget<File> for FileSystemTarget {
    async fn load_listing(root_path: &str, listing: Listing) -> Self {
        FileSystemTarget {
            root_directory: root_path.to_string(),
            listing: Arc::new(RwLock::new(listing)),
        }
    }
    async fn restore_object(&self, node: Node) -> HashMap<String, RestoreObject<File>> {
        let mut output = HashMap::new();
        // Get the actual path on the filesystem this refers to
        let root_path = Path::new(&self.root_directory);
        let rel_path = Path::new(&node.path);
        let path = root_path.join(rel_path);
        // FIXME: currently assumes that nodes are only files or direcotires
        if node.is_directory() {
            // If the node is a directory, just create it
            let path = path.to_owned();
            task::spawn_blocking(move || {
                create_dir_all(path).expect("Unable to create directory (restore_object)")
            })
            .await
            .expect("Unable to join blocking task");
            output
        } else {
            // Get the parent directory, and create it if it does not exist
            let parent_path = path
                .parent()
                .expect("Unable to get parent(restore_object)")
                .to_owned();
            task::spawn_blocking(move || {
                create_dir_all(parent_path).expect("Unable to create parent (restore_object)")
            })
            .await
            .expect("Unable to join blocking task");
            // Check to see if we have any extents
            if let Some(extents) = node.extents.as_ref() {
                // if the extents are empty, just touch the file and leave it
                if extents.is_empty() {
                    let path = path.to_owned();
                    task::spawn_blocking(move || File::create(path).expect("Unable to open file"))
                        .await
                        .expect("Unable to join blocking task");
                    output
                } else {
                    let mut file_object = RestoreObject::new(node.total_length);
                    for extent in extents {
                        file_object.direct_add_range(
                            extent.start,
                            extent.end,
                            File::create(path.clone()).expect("Unable to open file"),
                        );
                    }
                    output.insert(String::new(), file_object);
                    output
                }
            } else {
                let path = path.to_owned();
                task::spawn_blocking(move || File::create(path).expect("Unable to open file"))
                    .await
                    .expect("Unable to join blocking task");
                output
            }
        }
    }
    async fn restore_listing(&self) -> Listing {
        self.listing.read().await.clone()
    }
}

impl BackupDriver<File> for FileSystemTarget {}
impl RestoreDriver<File> for FileSystemTarget {}

#[cfg(test)]
mod tests {
    use super::*;
    use dir_diff;
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

    #[tokio::test(threaded_scheduler)]
    async fn backup_restore_structure() {
        let input_dir = make_test_directory();
        let root_path = input_dir.path().to_owned();

        let input_target = FileSystemTarget::new(&root_path.display().to_string());

        let listing = input_target.backup_paths().await;
        for node in listing {
            println!("Backing up: {}", node.path);
            input_target.backup_object(node).await;
        }

        let listing = input_target.backup_listing().await;
        println!("{:?}", listing);

        let output_dir = tempdir().unwrap();

        let output_target =
            FileSystemTarget::load_listing(&output_dir.path().display().to_string(), listing).await;

        let output_listing = output_target.restore_listing().await;
        for entry in output_listing {
            println!("Restore listing:");
            println!(" - {}", entry.path);
            output_target.restore_object(entry).await;
        }

        let _input_path = input_dir.path().display().to_string();
        let _output_path = output_dir.path().display().to_string();

        assert!(!dir_diff::is_different(&input_dir.path(), &output_dir.path()).unwrap());
    }
}
