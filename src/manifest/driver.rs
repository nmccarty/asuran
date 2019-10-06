use crate::chunker::Chunker;
use crate::manifest::archive::{Archive, Extent};
use crate::manifest::target::{BackupObject, BackupTarget, RestoreObject, RestoreTarget};
use crate::repository::{Backend, Repository};
use std::collections::HashMap;
use std::io::{Read, Write};

/// Collection of abstract methods for moving data from a storage target to a repository
///
/// This trait provides reasonable default versions of the functions for you
pub trait BackupDriver<T: Read>: BackupTarget<T> {
    /// Inserts an object into the repository using the output from BackupTarget::backup_object
    ///
    /// This method should only really be used directly when you want to change the data in route,
    /// otherwise use store_object.
    ///
    /// Stores objects in sub-namespaces of the namespace of the archive object provided
    fn raw_store_object(
        &self,
        repo: &mut Repository<impl Backend>,
        chunker: &Chunker,
        archive: &Archive,
        path: &str,
        objects: HashMap<String, BackupObject<T>>,
    ) -> Option<()> {
        for (namespace, backup_object) in objects {
            // TODO: Store total size in archive
            // let total_size = backup_object.total_size();
            // Get a new archive with the specified namespace
            let mut archive = archive.namespace_append(&namespace);
            // Pull ranges out of object and determine sparsity
            let mut ranges = backup_object.ranges();
            // Determine sparsity and load object into repository
            let range_count = ranges.len();
            if range_count == 0 {
                archive.put_empty(path);
            } else if range_count == 1 {
                let object = ranges.remove(0).object;
                archive.put_object(chunker, repo, path, object)?;
            } else {
                let mut readers: Vec<(Extent, T)> = Vec::new();
                for object in ranges.into_iter() {
                    let extent = Extent {
                        start: object.start as u64,
                        end: object.end as u64,
                    };
                    let object = object.object;
                    readers.push((extent, object));
                }
                archive.put_sparse_object(chunker, repo, path, readers)?;
            }
        }
        Some(())
    }

    /// Stores an object, performing the calls to BackupTarget::backup_object and raw_store_obejct
    /// for you.
    fn store_object(
        &self,
        repo: &mut Repository<impl Backend>,
        chunker: &Chunker,
        archive: &Archive,
        path: &str,
    ) -> Option<()> {
        let objects = self.backup_object(path);
        self.raw_store_object(repo, chunker, archive, path, objects)?;

        Some(())
    }
}
