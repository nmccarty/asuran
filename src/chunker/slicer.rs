//! A slicer cuts data into chunks based on some predefined method
//!
//! Most typical is content defined slicing, but format specific methods are also quite useful

/// Describes something that can slice objects in to chunks in a defined, repeatable manner
pub trait Slicer {}
