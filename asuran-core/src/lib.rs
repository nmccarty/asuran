/*!
This crate contains data structures that can be readily shared between both
synchronous and non-synchronous implementations of asuran.

When a data structure is present in this crate, and it has a
Serialize/Deserialize derive, the format that `rmp-serde` produces from
serializing that structure with the compact representation is considered to be
the iconically format of that objects on-disk representation.
*/

#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::similar_names)]
#![allow(clippy::pub_enum_variant_names)]
#![allow(clippy::missing_errors_doc)]

pub mod manifest;
pub mod repository;
