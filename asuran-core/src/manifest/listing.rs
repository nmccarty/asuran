//! This module contains structs for describing an object listing
//!
//! This can be thought of an abstract representation of a directory structure, but
//! it is not contained to only files or directories
use crate::manifest::archive::Extent;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The type of node in the listing
///
/// These names are more or less arbitrary, and they don't actually need to be files
/// or directory.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum NodeType {
    /// A node that has assocaited data and potentially associated metadata
    File,
    /// A node that has associated metadata, and no child nodes
    Link,
    /// A node that only has associated metadata, and potentially child nodes
    ///
    /// Contains the paths of any child members a node may have
    Directory { children: Vec<String> },
}

/// A node is a description of an object in the listing
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Node {
    /// The path of the object, in its orignal form before archive mangling
    ///
    /// Object paths are simply arbitrary strings
    pub path: String,
    /// The total length of the object, including holes in sparse objects
    pub total_length: u64,
    /// The total size of an object, not including holes in sparse files
    pub total_size: u64,
    /// The extents that make up a sparse object.
    ///
    /// This will be None if the object is not sparse.
    pub extents: Option<Vec<Extent>>,
    /// the type of the node
    pub node_type: NodeType,
}

impl Node {
    /// Returns true if the Node is a directory
    pub fn is_directory(&self) -> bool {
        match self.node_type {
            NodeType::Directory { .. } => true,
            _ => false,
        }
    }

    /// Returns true if the Node is a file
    pub fn is_file(&self) -> bool {
        match self.node_type {
            NodeType::File => true,
            _ => false,
        }
    }

    /// Returns a copy of self with any children (in a `NodeType::Directory`) removed
    pub fn drain_children(&self) -> Node {
        let node_type = match &self.node_type {
            NodeType::Directory { .. } => NodeType::Directory {
                children: Vec::new(),
            },
            x => x.clone(),
        };
        let mut new = self.clone();
        new.node_type = node_type;
        new
    }
}

/// Describes an abstract representation of a directory structure.
///
/// Conceptually represents the structure as a DAG
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Listing {
    /// Contains a mapping of paths to nodes.
    ///
    /// Two nodes are considered the same if they share the same path
    nodes: HashMap<String, Node>,
    /// Contains the paths of the nodes in the 'root directory' of this listing
    root: Vec<String>,
}

impl Listing {
    /// Adds a child to the directory with the specified path
    ///
    /// Will do nothing if the given directory does not exist or is not a directory
    ///
    /// If the parent path is empty, will add it to the "children of the root node"
    /// entry
    pub fn add_child(&mut self, path: &str, child: Node) {
        if path.is_empty() {
            self.root.push(child.path.clone());
            self.nodes.insert(child.path.clone(), child);
        } else {
            let parent = match self.nodes.get_mut(path) {
                Some(parent) => parent,
                _ => return,
            };
            match &mut parent.node_type {
                NodeType::Directory { children } => {
                    children.push(child.path.clone());
                }
                _ => return,
            }

            self.nodes.insert(child.path.clone(), child);
        }
    }

    /// Creates a by-reference iterator over the Nodes in this listing
    pub fn iter(&self) -> RefListingIterator<'_> {
        self.into_iter()
    }
}

/// Iterates over an owned `Listing`
///
/// Does so in breadth-first order
pub struct ListingIterator {
    /// The nodes currently being offered up for iteration
    node_buffer: Vec<Node>,
    /// The children of nodes already consumed
    children_buffer: Vec<Node>,
    /// Map containing the remaining nodes
    node_map: HashMap<String, Node>,
}

impl Iterator for ListingIterator {
    type Item = Node;
    fn next(&mut self) -> Option<Self::Item> {
        // Check if the node buffer is empty, if not we will need to refill it from the
        // children buffer
        if self.node_buffer.is_empty() {
            self.node_buffer = self.children_buffer.drain(..).collect();
        }
        // If the node buffer is empty after this, we are out of nodes
        let next = self.node_buffer.pop()?;
        // If it is a directory, add its children to the children buffer
        if let NodeType::Directory { children } = &next.node_type {
            for child_path in children {
                // Get the node out of the node_map
                let child = self.node_map.remove(child_path);
                if let Some(child) = child {
                    self.children_buffer.push(child)
                };
            }
        }
        Some(next)
    }
}

impl IntoIterator for Listing {
    type Item = Node;
    type IntoIter = ListingIterator;
    fn into_iter(mut self) -> Self::IntoIter {
        let mut node_buffer = Vec::new();
        for path in self.root {
            let node = self.nodes.remove(&path).expect("Invalid path in listing!");
            node_buffer.push(node);
        }
        ListingIterator {
            node_buffer,
            children_buffer: Vec::new(),
            node_map: self.nodes,
        }
    }
}

/// Iterated over a borrowed `Listing`
///
/// Does so in breadth-first order
pub struct RefListingIterator<'a> {
    node_buffer: Vec<&'a Node>,
    children_buffer: Vec<&'a Node>,
    node_map: HashMap<String, &'a Node>,
}

impl<'a> Iterator for RefListingIterator<'a> {
    type Item = &'a Node;
    fn next(&mut self) -> Option<Self::Item> {
        // Check if the node buffer is empty, if not we will need to refill it from the
        // children buffer
        if self.node_buffer.is_empty() {
            self.node_buffer = self.children_buffer.drain(..).collect();
        }
        // If the node buffer is empty after this, we are out of nodes
        let next = self.node_buffer.pop()?;
        // If it is a directory, add its children to the children buffer
        if let NodeType::Directory { children } = &next.node_type {
            for child_path in children {
                // Get the node out of the node_map
                let child = self.node_map.remove(child_path);
                if let Some(child) = child {
                    self.children_buffer.push(child);
                }
            }
        }
        Some(next)
    }
}

impl<'a> IntoIterator for &'a Listing {
    type Item = &'a Node;
    type IntoIter = RefListingIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        let mut node_map = self
            .nodes
            .iter()
            .map(|(x, y)| (x.clone(), y))
            .collect::<HashMap<String, _>>();
        let mut node_buffer = Vec::new();
        for path in &self.root {
            let node = node_map.remove(path).expect("Invalid path in listing!");
            node_buffer.push(node);
        }
        RefListingIterator {
            node_buffer,
            children_buffer: Vec::new(),
            node_map,
        }
    }
}
