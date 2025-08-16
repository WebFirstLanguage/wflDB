//! Simplified HTTP server for Phase 0 spike - Fixed for hyper 0.14

use hyper::{Body, Request, Response, Server, Method, StatusCode};
use hyper::service::{make_service_fn, service_fn};
use std::net::SocketAddr;
use std::convert::Infallible;
use std::sync::Arc;
use tracing::{error, info, debug};
use wfldb_core::*;
use wfldb_engine::{StorageEngine, Storage};

pub struct SimpleServer {
    storage: StorageEngine,
}

impl SimpleServer {
    pub fn new(storage: StorageEngine) -> Self {
        Self { storage }
    }

    pub async fn serve(self, addr: SocketAddr) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let storage = Arc::new(self.storage);
        
        let make_svc = make_service_fn(move |_conn| {
            let storage = storage.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    handle_request(req, storage.clone())
                }))
            }
        });

        let server = Server::bind(&addr).serve(make_svc);

        info!("wflDB server listening on {}", addr);

        if let Err(e) = server.await {
            error!("Server error: {}", e);
            return Err(Box::new(e));
        }

        Ok(())
    }
}

/// Simple request handler for spike
async fn handle_request(
    req: Request<Body>,
    storage_engine: Arc<StorageEngine>,
) -> std::result::Result<Response<Body>, Infallible> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path();
    
    debug!("Handling {} {}", method, path);

    let storage = Storage::new((*storage_engine).clone());

    let result: std::result::Result<Response<Body>, Infallible> = match (&method, path) {
        // Health check endpoint
        (&Method::GET, "/health") => {
            let response_body = r#"{"status":"healthy","version":"0.1.0","service":"wfldb"}"#;
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(Body::from(response_body))
                .unwrap())
        }
        
        // Echo endpoint for testing
        (&Method::POST, "/echo") => {
            match hyper::body::to_bytes(req.into_body()).await {
                Ok(body_bytes) => {
                    let echo_response = format!(
                        r#"{{"echo":"{}","size":{},"timestamp":"{}"}}"#,
                        String::from_utf8_lossy(&body_bytes),
                        body_bytes.len(),
                        chrono::Utc::now().to_rfc3339()
                    );
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .body(Body::from(echo_response))
                        .unwrap())
                }
                Err(_) => {
                    let error_response = r#"{"error":"Failed to read request body"}"#;
                    Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("content-type", "application/json")
                        .body(Body::from(error_response))
                        .unwrap())
                }
            }
        }
        
        // Object storage endpoints
        (&Method::PUT, path) if path.starts_with("/v1/") => {
            match parse_object_path(path) {
                Ok((bucket_id, key)) => {
                    match hyper::body::to_bytes(req.into_body()).await {
                        Ok(body_bytes) => {
                            match storage.put_object(&bucket_id, &key, &body_bytes) {
                                Ok(metadata) => {
                                    let response = format!(
                                        r#"{{"success":true,"bucket":"{}","key":"{}","size":{},"version":"{}","chunked":{}}}"#,
                                        bucket_id.as_str(),
                                        key.as_str(),
                                        metadata.size,
                                        metadata.version.to_string(),
                                        metadata.is_chunked()
                                    );
                                    Ok(Response::builder()
                                        .status(StatusCode::CREATED)
                                        .header("content-type", "application/json")
                                        .body(Body::from(response))
                                        .unwrap())
                                }
                                Err(e) => {
                                    let error_response = format!(r#"{{"error":"{}"}}"#, e);
                                    Ok(Response::builder()
                                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                                        .header("content-type", "application/json")
                                        .body(Body::from(error_response))
                                        .unwrap())
                                }
                            }
                        }
                        Err(_) => {
                            let error_response = r#"{"error":"Failed to read request body"}"#;
                            Ok(Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .header("content-type", "application/json")
                                .body(Body::from(error_response))
                                .unwrap())
                        }
                    }
                }
                Err(e) => {
                    let error_response = format!(r#"{{"error":"{}"}}"#, e);
                    Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("content-type", "application/json")
                        .body(Body::from(error_response))
                        .unwrap())
                }
            }
        }
        
        (&Method::GET, path) if path.starts_with("/v1/") => {
            match parse_object_path(path) {
                Ok((bucket_id, key)) => {
                    match storage.get_object(&bucket_id, &key) {
                        Ok(Some(data)) => {
                            Ok(Response::builder()
                                .status(StatusCode::OK)
                                .header("content-type", "application/octet-stream")
                                .header("content-length", data.len().to_string())
                                .body(Body::from(data))
                                .unwrap())
                        }
                        Ok(None) => {
                            let error_response = r#"{"error":"Object not found"}"#;
                            Ok(Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .header("content-type", "application/json")
                                .body(Body::from(error_response))
                                .unwrap())
                        }
                        Err(e) => {
                            let error_response = format!(r#"{{"error":"{}"}}"#, e);
                            Ok(Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .header("content-type", "application/json")
                                .body(Body::from(error_response))
                                .unwrap())
                        }
                    }
                }
                Err(e) => {
                    let error_response = format!(r#"{{"error":"{}"}}"#, e);
                    Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("content-type", "application/json")
                        .body(Body::from(error_response))
                        .unwrap())
                }
            }
        }
        
        (&Method::DELETE, path) if path.starts_with("/v1/") => {
            match parse_object_path(path) {
                Ok((bucket_id, key)) => {
                    match storage.delete_object(&bucket_id, &key) {
                        Ok(()) => {
                            let response = format!(
                                r#"{{"success":true,"bucket":"{}","key":"{}","deleted":true}}"#,
                                bucket_id.as_str(),
                                key.as_str()
                            );
                            Ok(Response::builder()
                                .status(StatusCode::OK)
                                .header("content-type", "application/json")
                                .body(Body::from(response))
                                .unwrap())
                        }
                        Err(e) => {
                            let error_response = format!(r#"{{"error":"{}"}}"#, e);
                            Ok(Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .header("content-type", "application/json")
                                .body(Body::from(error_response))
                                .unwrap())
                        }
                    }
                }
                Err(e) => {
                    let error_response = format!(r#"{{"error":"{}"}}"#, e);
                    Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("content-type", "application/json")
                        .body(Body::from(error_response))
                        .unwrap())
                }
            }
        }
        
        // Not found
        _ => {
            let error_response = r#"{"error":"Not found"}"#;
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("content-type", "application/json")
                .body(Body::from(error_response))
                .unwrap())
        }
    };

    match result {
        Ok(response) => {
            info!("{} {} -> {}", method, path, response.status());
            Ok(response)
        }
        Err(e) => {
            error!("Handler error for {} {}: {}", method, path, e);
            let error_response = r#"{"error":"Internal server error"}"#;
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("content-type", "application/json")
                .body(Body::from(error_response))
                .unwrap())
        }
    }
}

/// Parse object path like "/v1/bucket/key" into bucket and key
fn parse_object_path(path: &str) -> std::result::Result<(BucketId, Key), String> {
    let parts: Vec<&str> = path.strip_prefix("/v1/")
        .unwrap_or("")
        .split('/')
        .collect();
    
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err("Invalid path format. Expected /v1/{bucket}/{key}".to_string());
    }
    
    let bucket_id = BucketId::new(parts[0])
        .map_err(|_| "Invalid bucket name".to_string())?;
    
    let key_part = parts[1..].join("/"); // Support nested keys
    let key = Key::new(&key_part)
        .map_err(|_| "Invalid key".to_string())?;
    
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