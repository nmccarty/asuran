use crate::chunker::{Chunker, SlicerSettings};
use crate::manifest::archive::{Archive, Extent};
use crate::manifest::target::{BackupObject, BackupTarget, RestoreObject, RestoreTarget};
use crate::repository::{Backend, Repository};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::io::{Empty, Read, Write};

/// Collection of abstract methods for moving data from a storage target to a repository
///
/// This trait provides reasonable default versions of the functions for you
#[async_trait]
pub trait BackupDriver<T: Read + Send + 'static>: BackupTarget<T> {
    /// Inserts an object into the repository using the output from BackupTarget::backup_object
    ///
    /// This method should only really be used directly when you want to change the data in route,
    /// otherwise use store_object.
    ///
    /// Stores objects in sub-namespaces of the namespace of the archive object provided
    async fn raw_store_object<
        B: Backend,
        C: SlicerSettings<Empty> + SlicerSettings<T> + 'static,
    >(
        &self,
        repo: &mut Repository<B>,
        chunker: Chunker<C>,
        archive: &Archive,
        path: String,
        objects: HashMap<String, BackupObject<T>>,
    ) -> Result<()> {
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
                archive.put_empty(&path);
            } else if range_count == 1 {
                let object = ranges.remove(0).object;
                archive.put_object(&chunker, repo, &path, object).await?;
            } else {
                let mut readers: Vec<(Extent, T)> = Vec::new();
                for object in ranges {
                    let extent = Extent {
                        start: object.start,
                        end: object.end,
                    };
                    let object = object.object;
                    readers.push((extent, object));
                }
                archive
                    .put_sparse_object(&chunker, repo, &path, readers)
                    .await?;
            }
        }
        Ok(())
    }

    /// Stores an object, performing the calls to BackupTarget::backup_object and raw_store_obejct
    /// for you.
    async fn store_object<B: Backend, C: SlicerSettings<Empty> + SlicerSettings<T> + 'static>(
        &self,
        repo: &mut Repository<B>,
        chunker: Chunker<C>,
        archive: &Archive,
        path: String,
    ) -> Result<()> {
        let objects = self.backup_object(&path);
        self.raw_store_object(repo, chunker, archive, path, objects)
            .await
    }
}

/// Collection of abstract methods for moving data from a storage target to a reposiotry
///
/// This trait provides resasonable default versions for you
#[async_trait(?Send)]
pub trait RestoreDriver<T: Write + Send + 'static>: RestoreTarget<T> {
    /// Retrives an object from the repository using the output from RestoreTarget::restore_object
    ///
    /// This method should really only be used directly when you want to change the data in route,
    /// otherwise use retrive_object.
    ///
    /// Retrives objects from the stub-namespaces of the namespace of the object provided
    async fn raw_retrieve_object<B: Backend>(
        &self,
        repo: &Repository<B>,
        archive: &Archive,
        path: &str,
        objects: HashMap<String, RestoreObject<T>>,
    ) -> Result<()> {
        for (namespace, restore_object) in objects {
            // TODO: get total size and do something with it
            // Get a new archive with the specified namespace
            let archive = archive.namespace_append(&namespace);
            // Pull ranges out of object and determine sparsity
            let mut ranges = restore_object.ranges();
            // determin sparsity and retrieve object from repository
            let range_count = ranges.len();
            // This does not have a case for zero, as the target method should have already created
            // an empty object
            if range_count == 1 {
                let object = ranges.remove(0).object;
                archive.get_object(&repo, &path, object).await?;
            } else if range_count > 1 {
                let mut writers: Vec<(Extent, T)> = Vec::new();
                for object in ranges {
                    let extent = Extent {
                        start: object.start,
                        end: object.end,
                    };
                    let object = object.object;
                    writers.push((extent, object));
                }
                archive.get_sparse_object(&repo, &path, writers).await?;
            }
        }
        Ok(())
    }

    /// Retrieves an object, performing the call to BackupTarget::restore_object and raw_retrive_object
    /// for you.
    async fn retrieve_object<B: Backend>(
        &self,
        repo: &Repository<B>,
        archive: &Archive,
        path: &str,
    ) -> Result<()> {
        let objects = self.restore_object(path);
        self.raw_retrieve_object(repo, archive, path, objects).await
    }
}
