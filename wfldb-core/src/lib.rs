//! Core data models and types for wflDB


pub mod error;
pub mod types;

pub use error::*;
pub use types::*;

/// Result type alias for wflDB operations
pub type Result<T> = std::result::Result<T, WflDBError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_id_creation() {
        let bucket = BucketId::new("test-bucket").unwrap();
        assert_eq!(bucket.as_str(), "test-bucket");
    }

    #[test]
    fn test_bucket_id_validation() {
        // Valid bucket names
        assert!(BucketId::new("bucket").is_ok());
        assert!(BucketId::new("bucket-123").is_ok());
        assert!(BucketId::new("bucket_123").is_ok());
        
        // Invalid bucket names
        assert!(BucketId::new("").is_err());
        assert!(BucketId::new("bucket with spaces").is_err());
        assert!(BucketId::new("bucket/with/slashes").is_err());
    }

    #[test]
    fn test_object_metadata() {
        let metadata = ObjectMetadata {
            size: 1024,
            version: Version::new(),
            content_hash: Some(ContentHash::new(b"test data")),
            created_at: std::time::SystemTime::now(),
            chunk_manifest: None,
        };
        
        assert_eq!(metadata.size, 1024);
        assert!(metadata.content_hash.is_some());
    }
}