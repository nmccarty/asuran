use crate::chunker::AsyncChunker;
use crate::manifest::archive::{ActiveArchive, Extent};
use crate::manifest::target::{BackupObject, BackupTarget, RestoreObject, RestoreTarget};
use crate::repository::{BackendClone, Repository};
use async_trait::async_trait;
use std::collections::HashMap;
use std::io::{Read, Write};
use thiserror::Error;

use asuran_core::manifest::listing::*;

/// An error for things that can go wrong with drivers
#[derive(Error, Debug)]
pub enum DriverError {
    #[error("")]
    ArchiveError(#[from] crate::manifest::archive::ArchiveError),
}

type Result<T> = std::result::Result<T, DriverError>;

/// Defines a type that can, semi-automatically, drive the storage of objects from
/// an associated `BackupTarget` into a repository.
///
/// As this is effectively an extension trait for a `BackupTarget`, and the behavior
/// will usually be more or less the same, reasonable default implementations have
/// been provided.
#[async_trait]
pub trait BackupDriver<T: Read + Send + 'static>: BackupTarget<T> {
    /// Inserts an object into the repository using the output from
    /// `BackupTarget::backup_object`
    ///
    /// This method should only be used directly when you want to modify the data in
    /// route, otherwise use `store_object`.
    ///
    /// Stores objects in sub-namespaces of the namespace of the archive object provided
    async fn raw_store_object<B: BackendClone, C: AsyncChunker + Send + 'static>(
        &self,
        repo: &mut Repository<B>,
        chunker: C,
        archive: &ActiveArchive,
        node: Node,
        objects: HashMap<String, BackupObject<T>>,
    ) -> Result<()> {
        if node.is_file() {
            for (namespace, backup_object) in objects {
                let path = &node.path;
                // TODO (#45): Store total size in archive
                // let total_size = backup_object.total_size();
                // Get a new archive with the specified namespace
                let mut archive = archive.namespace_append(&namespace);
                // Pull ranges out of object and determine sparsity
                let mut ranges = backup_object.ranges();
                // Determine sparsity and load object into repository
                let range_count = ranges.len();
                if range_count == 0 {
                    archive.put_empty(path).await;
                } else if range_count == 1 {
                    let object = ranges.remove(0).object;
                    archive.put_object(&chunker, repo, path, object).await?;
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
                        .put_sparse_object(&chunker, repo, path, readers)
                        .await?;
                }
            }
        }
        Ok(())
    }

    /// Convenience method that performs a call to `self.backup_object` for you and
    /// routes the results into `self.raw_store_object`
    async fn store_object<B: BackendClone, C: AsyncChunker + Send + 'static>(
        &self,
        repo: &mut Repository<B>,
        chunker: C,
        archive: &ActiveArchive,
        node: Node,
    ) -> Result<()> {
        let objects = self.backup_object(node.clone()).await;
        self.raw_store_object(repo, chunker, archive, node, objects)
            .await
    }
}

/// Defines a type that can, semi-automatically, drive the retrieval of objects from
/// a repository into an associated `RestoreTarget`.
///
/// As this is effectively an extension trait for a `RestoreTarget`, and the
/// behavior will usually be more or less the same, reasonable default
/// implementations have been provided.
#[async_trait]
pub trait RestoreDriver<T: Write + Send + 'static>: RestoreTarget<T> {
    /// Retrives an object from the repository using the output from RestoreTarget::restore_object
    ///
    /// This method should really only be used directly when you want to change the data in route,
    /// otherwise use retrive_object.
    ///
    /// Retrives objects from the stub-namespaces of the namespace of the object provided
    async fn raw_retrieve_object<B: BackendClone>(
        &self,
        repo: &mut Repository<B>,
        archive: &ActiveArchive,
        node: Node,
        objects: HashMap<String, RestoreObject<T>>,
    ) -> Result<()> {
        let path = &node.path;
        if node.is_file() {
            for (namespace, restore_object) in objects {
                // TODO (#45): get total size and do something with it
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
                    archive.get_object(repo, &path, object).await?;
                // This used to be a if range count > 1, this may cause issues
                } else {
                    let mut writers: Vec<(Extent, T)> = Vec::new();
                    for object in ranges {
                        let extent = Extent {
                            start: object.start,
                            end: object.end,
                        };
                        let object = object.object;
                        writers.push((extent, object));
                    }
                    archive.get_sparse_object(repo, &path, writers).await?;
                }
            }
        }
        Ok(())
    }

    /// Retrieves an object, performing the call to BackupTarget::restore_object and raw_retrive_object
    /// for you.
    async fn retrieve_object<B: BackendClone>(
        &self,
        repo: &mut Repository<B>,
        archive: &ActiveArchive,
        node: Node,
    ) -> Result<()> {
        let objects = self.restore_object(node.clone()).await;
        self.raw_retrieve_object(repo, archive, node, objects).await
    }
}
