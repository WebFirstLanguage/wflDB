//! Client error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Request failed: {0}")]
    Request(String),
    
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    
    #[error("Stream error: {0}")]
    Stream(String),
    
    #[error("Multipart upload error: {0}")]
    MultipartUpload(String),
    
    #[error("Core error: {0}")]
    Core(#[from] wfldb_core::WflDBError),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("HTTP error: {0}")]
    Http(String),
}