use serde::{Deserialize, Serialize};

use crate::manifest::archive::Extent;

#[derive(Serialize, Deserialize, Clone, Debug)]
/// A single node in a listing
///
/// Describes the path of an object, as well as its size and sparsity
pub struct Node {
    /// The path of the object
    ///
    /// Does not include the namespace
    path: String,
    /// The size of the object, including holes
    total_size: u64,
    /// The size of the object, not including holes
    sparse_size: u64,
    /// If the object is sparse, this will contain an list of the extents
    ///
    /// Will be none otherwise
    extents: Option<Vec<Extent>>,
}

impl Node {
    pub fn new(path: String, total_size: u64, sparse_size: u64, sparsity: bool) -> Node {
        Node {
            path,
            total_size,
            sparse_size,
            extents: if sparsity { Some(Vec::new()) } else { None },
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    pub fn sparse_size(&self) -> u64 {
        self.sparse_size
    }

    pub fn is_sparse(&self) -> bool {
        self.extents.is_some()
    }
}

/// A subset of a listing identified by a colon delimited set of strings

#[derive(Serialize, Deserialize, Clone, Debug)]
/// The listing of objects
pub struct Listing {}
