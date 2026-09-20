#![forbid(unsafe_code)]

pub mod module;
pub mod protocol;
pub mod service;
pub mod storage;

pub use module::{manifest, DiscussionsHandler};
pub use storage::{DiscussionsSnapshot, Storage, StorageError};
