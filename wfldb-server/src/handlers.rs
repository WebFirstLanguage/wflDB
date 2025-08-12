//! HTTP request handlers for wflDB server

use hyper::{Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use serde_json::json;
use std::collections::HashMap;
use tracing::{debug, error, info};
use wfldb_core::*;
use wfldb_engine::{StorageEngine, Storage};
use crate::server::{simple_response, streaming_response};

type BoxBody = http_body_util::Full<bytes::Bytes>;

/// Main request handler
pub async fn handle_request(
    req: Request<hyper::body::Incoming>,
    storage_engine: StorageEngine,
) -> Result<Response<BoxBody>, hyper::Error> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path();
    
    debug!("Handling {} {}", method, path);

    let storage = Storage::new(storage_engine);

    let result = match (&method, path) {
        // Health check endpoint
        (Method::GET, "/health") => handle_health().await,
        
        // Echo endpoint for testing
        (Method::POST, "/echo") => handle_echo(req).await,
        
        // Object storage endpoints
        (Method::PUT, path) if path.starts_with("/v1/") => {
            handle_put_object(req, storage, path).await
        }
        (Method::GET, path) if path.starts_with("/v1/") => {
            handle_get_object(storage, path).await
        }
        (Method::DELETE, path) if path.starts_with("/v1/") => {
            handle_delete_object(storage, path).await
        }
        
        // Not found
        _ => {
            simple_response(
                StatusCode::NOT_FOUND,
                json!({"error": "Not found"}).to_string()
            )
        }
    };

    match result {
        Ok(response) => {
            info!("{} {} -> {}", method, path, response.status());
            Ok(response)
        }
        Err(e) => {
            error!("Handler error for {} {}: {}", method, path, e);
            simple_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "Internal server error"}).to_string()
            )
        }
    }
}

/// Health check handler
async fn handle_health() -> Result<Response<BoxBody>, hyper::Error> {
    simple_response(
        StatusCode::OK,
        json!({
            "status": "healthy",
            "version": "0.1.0",
            "service": "wfldb"
        }).to_string()
    )
}

/// Echo handler for testing
async fn handle_echo(req: Request<hyper::body::Incoming>) -> Result<Response<BoxBody>, hyper::Error> {
    let body_bytes = req.collect().await?.to_bytes();
    
    let echo_response = json!({
        "echo": String::from_utf8_lossy(&body_bytes),
        "size": body_bytes.len(),
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    simple_response(StatusCode::OK, echo_response.to_string())
}

/// PUT object handler
async fn handle_put_object(
    req: Request<hyper::body::Incoming>,
    storage: Storage,
    path: &str,
) -> Result<Response<BoxBody>, hyper::Error> {
    let (bucket_id, key) = parse_object_path(path)?;
    
    // Read request body
    let body_bytes = req.collect().await?.to_bytes();
    
    debug!("Putting object: bucket={}, key={}, size={}", 
           bucket_id.as_str(), key.as_str(), body_bytes.len());

    // Store object
    match storage.put_object(&bucket_id, &key, &body_bytes) {
        Ok(metadata) => {
            let response = json!({
                "success": true,
                "bucket": bucket_id.as_str(),
                "key": key.as_str(),
                "size": metadata.size,
                "version": metadata.version.to_string(),
                "chunked": metadata.is_chunked()
            });
            
            simple_response(StatusCode::CREATED, response.to_string())
        }
        Err(e) => {
            error!("Failed to put object: {}", e);
            simple_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": e.to_string()}).to_string()
            )
        }
    }
}

/// GET object handler
async fn handle_get_object(
    storage: Storage,
    path: &str,
) -> Result<Response<BoxBody>, hyper::Error> {
    let (bucket_id, key) = parse_object_path(path)?;
    
    debug!("Getting object: bucket={}, key={}", bucket_id.as_str(), key.as_str());

    match storage.get_object(&bucket_id, &key) {
        Ok(Some(data)) => {
            info!("Retrieved object: bucket={}, key={}, size={}", 
                  bucket_id.as_str(), key.as_str(), data.len());
            streaming_response(data)
        }
        Ok(None) => {
            simple_response(
                StatusCode::NOT_FOUND,
                json!({"error": "Object not found"}).to_string()
            )
        }
        Err(e) => {
            error!("Failed to get object: {}", e);
            simple_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": e.to_string()}).to_string()
            )
        }
    }
}

/// DELETE object handler
async fn handle_delete_object(
    storage: Storage,
    path: &str,
) -> Result<Response<BoxBody>, hyper::Error> {
    let (bucket_id, key) = parse_object_path(path)?;
    
    debug!("Deleting object: bucket={}, key={}", bucket_id.as_str(), key.as_str());

    match storage.delete_object(&bucket_id, &key) {
        Ok(()) => {
            let response = json!({
                "success": true,
                "bucket": bucket_id.as_str(),
                "key": key.as_str(),
                "deleted": true
            });
            
            simple_response(StatusCode::OK, response.to_string())
        }
        Err(e) => {
            error!("Failed to delete object: {}", e);
            simple_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": e.to_string()}).to_string()
            )
        }
    }
}

/// Parse object path like "/v1/bucket/key" into bucket and key
fn parse_object_path(path: &str) -> Result<(BucketId, Key), hyper::Error> {
    let parts: Vec<&str> = path.strip_prefix("/v1/")
        .unwrap_or("")
        .split('/')
        .collect();
    
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(hyper::Error::from(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid path format. Expected /v1/{bucket}/{key}"
        )));
    }
    
    let bucket_id = BucketId::new(parts[0])
        .map_err(|_| hyper::Error::from(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid bucket name"
        )))?;
    
    let key_part = parts[1..].join("/"); // Support nested keys
    let key = Key::new(&key_part)
        .map_err(|_| hyper::Error::from(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid key"
        )))?;
    
    Ok((bucket_id, key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_object_path() {
        // Valid paths
        let (bucket, key) = parse_object_path("/v1/photos/cat.jpg").unwrap();
        assert_eq!(bucket.as_str(), "photos");
        assert_eq!(key.as_str(), "cat.jpg");

        let (bucket, key) = parse_object_path("/v1/documents/folder/file.txt").unwrap();
        assert_eq!(bucket.as_str(), "documents");
        assert_eq!(key.as_str(), "folder/file.txt");

        // Invalid paths
        assert!(parse_object_path("/v1/").is_err());
        assert!(parse_object_path("/v1/bucket/").is_err());
        assert!(parse_object_path("/v1//key").is_err());
    }
}