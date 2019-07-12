//! libasuran provides a consistent high-level interface to asuran archives
//! across multiple storage backends and backup targets.
//!
//! Asuran allows for the storing of multiple rich archives in a single repository,
//! while providing encryption, compression, and global deduplication.

pub mod chunker;
pub mod manifest;
pub mod repository;
