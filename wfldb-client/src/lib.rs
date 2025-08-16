//! wflDB Rust client SDK

use std::sync::Arc;
use wfldb_core::*;

pub mod client;
pub mod error;
pub mod multipart;
pub mod streaming;

pub use client::Client;
pub use error::ClientError;
pub use multipart::MultipartUpload;
pub use streaming::{StreamingGet, StreamingPut};

pub type Result<T> = std::result::Result<T, ClientError>;