#![doc = include_str!("../README.md")]

/// RPM header values, tag identifiers, and lossless raw header access.
pub mod header;
/// The historical RPM lead, which does not identify the package format alone.
pub mod lead;
/// Streaming newc/crc CPIO archive creation and extraction.
pub mod payload;
/// Package reading, creation, editing, comparison, linting, and target manifests.
pub mod rpm;

pub(crate) mod utils;
pub use rpm::*;
