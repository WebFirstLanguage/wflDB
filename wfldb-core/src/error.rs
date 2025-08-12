//! Error types for wflDB

use thiserror::Error;

#[derive(Error, Debug)]
pub enum WflDBError {
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Invalid bucket name: {0}")]
    InvalidBucketName(String),
    
    #[error("Invalid key: {0}")]
    InvalidKey(String),
    
    #[error("Object not found: {key}")]
    ObjectNotFound { key: String },
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Internal error: {0}")]
    Internal(String),
}