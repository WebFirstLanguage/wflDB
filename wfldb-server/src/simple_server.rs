//! Simplified HTTP server for Phase 0 spike

use hyper::server::conn::http2;
use hyper::service::service_fn;
use hyper::{Request, Response, Method, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use http_body_util::{BodyExt, Full};
use tokio::net::{TcpListener, TcpStream};
use std::net::SocketAddr;
use std::convert::Infallible;
use tracing::{error, info, debug};
use wfldb_core::*;
use wfldb_engine::{StorageEngine, Storage};

type BoxBody = Full<bytes::Bytes>;

pub struct SimpleServer {
    storage: StorageEngine,
}

impl SimpleServer {
    pub fn new(storage: StorageEngine) -> Self {
        Self { storage }
    }

    pub async fn serve(self, addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(addr).await?;
        info!("wflDB server listening on {}", addr);

        loop {
            let (stream, remote_addr) = listener.accept().await?;
            debug!("New connection from {}", remote_addr);

            let storage = self.storage.clone();
            tokio::spawn(async move {
                if let Err(err) = Self::handle_connection(stream, storage).await {
                    error!("Connection error from {}: {}", remote_addr, err);
                }
            });
        }
    }

    async fn handle_connection(
        stream: TcpStream,
        storage: StorageEngine,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let io = TokioIo::new(stream);

        let service = service_fn(move |req| {
            let storage = storage.clone();
            async move { handle_request(req, storage).await }
        });

        if let Err(err) = http2::Builder::new(TokioExecutor::new())
            .serve_connection(io, service)
            .await
        {
            error!("HTTP/2 connection error: {}", err);
        }

        Ok(())
    }
}

/// Simple request handler for spike
async fn handle_request(
    req: Request<hyper::body::Incoming>,
    storage_engine: StorageEngine,
) -> Result<Response<BoxBody>, Infallible> {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path();
    
    debug!("Handling {} {}", method, path);

    let storage = Storage::new(storage_engine);

    let result = match (&method, path) {
        // Health check endpoint
        (Method::GET, "/health") => {
            let response_body = r#"{"status":"healthy","version":"0.1.0","service":"wfldb"}"#;
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/json")
                .body(Full::new(bytes::Bytes::from(response_body)))
                .unwrap())
        }
        
        // Echo endpoint for testing
        (Method::POST, "/echo") => {
            match req.collect().await {
                Ok(body) => {
                    let body_bytes = body.to_bytes();
                    let echo_response = format!(
                        r#"{{"echo":"{}","size":{},"timestamp":"{}"}}"#,
                        String::from_utf8_lossy(&body_bytes),
                        body_bytes.len(),
                        chrono::Utc::now().to_rfc3339()
                    );
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .body(Full::new(bytes::Bytes::from(echo_response)))
                        .unwrap())
                }
                Err(_) => {
                    let error_response = r#"{"error":"Failed to read request body"}"#;
                    Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("content-type", "application/json")
                        .body(Full::new(bytes::Bytes::from(error_response)))
                        .unwrap())
                }
            }
        }
        
        // Object storage endpoints
        (Method::PUT, path) if path.starts_with("/v1/") => {
            match parse_object_path(path) {
                Ok((bucket_id, key)) => {
                    match req.collect().await {
                        Ok(body) => {
                            let body_bytes = body.to_bytes();
                            
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
                                        .body(Full::new(bytes::Bytes::from(response)))
                                        .unwrap())
                                }
                                Err(e) => {
                                    let error_response = format!(r#"{{"error":"{}"}}"#, e);
                                    Ok(Response::builder()
                                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                                        .header("content-type", "application/json")
                                        .body(Full::new(bytes::Bytes::from(error_response)))
                                        .unwrap())
                                }
                            }
                        }
                        Err(_) => {
                            let error_response = r#"{"error":"Failed to read request body"}"#;
                            Ok(Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .header("content-type", "application/json")
                                .body(Full::new(bytes::Bytes::from(error_response)))
                                .unwrap())
                        }
                    }
                }
                Err(e) => {
                    let error_response = format!(r#"{{"error":"{}"}}"#, e);
                    Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("content-type", "application/json")
                        .body(Full::new(bytes::Bytes::from(error_response)))
                        .unwrap())
                }
            }
        }
        
        (Method::GET, path) if path.starts_with("/v1/") => {
            match parse_object_path(path) {
                Ok((bucket_id, key)) => {
                    match storage.get_object(&bucket_id, &key) {
                        Ok(Some(data)) => {
                            Ok(Response::builder()
                                .status(StatusCode::OK)
                                .header("content-type", "application/octet-stream")
                                .header("content-length", data.len().to_string())
                                .body(Full::new(bytes::Bytes::from(data)))
                                .unwrap())
                        }
                        Ok(None) => {
                            let error_response = r#"{"error":"Object not found"}"#;
                            Ok(Response::builder()
                                .status(StatusCode::NOT_FOUND)
                                .header("content-type", "application/json")
                                .body(Full::new(bytes::Bytes::from(error_response)))
                                .unwrap())
                        }
                        Err(e) => {
                            let error_response = format!(r#"{{"error":"{}"}}"#, e);
                            Ok(Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .header("content-type", "application/json")
                                .body(Full::new(bytes::Bytes::from(error_response)))
                                .unwrap())
                        }
                    }
                }
                Err(e) => {
                    let error_response = format!(r#"{{"error":"{}"}}"#, e);
                    Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("content-type", "application/json")
                        .body(Full::new(bytes::Bytes::from(error_response)))
                        .unwrap())
                }
            }
        }
        
        (Method::DELETE, path) if path.starts_with("/v1/") => {
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
                                .body(Full::new(bytes::Bytes::from(response)))
                                .unwrap())
                        }
                        Err(e) => {
                            let error_response = format!(r#"{{"error":"{}"}}"#, e);
                            Ok(Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .header("content-type", "application/json")
                                .body(Full::new(bytes::Bytes::from(error_response)))
                                .unwrap())
                        }
                    }
                }
                Err(e) => {
                    let error_response = format!(r#"{{"error":"{}"}}"#, e);
                    Ok(Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("content-type", "application/json")
                        .body(Full::new(bytes::Bytes::from(error_response)))
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
                .body(Full::new(bytes::Bytes::from(error_response)))
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
                .body(Full::new(bytes::Bytes::from(error_response)))
                .unwrap())
        }
    }
}

/// Parse object path like "/v1/bucket/key" into bucket and key
fn parse_object_path(path: &str) -> Result<(BucketId, Key), String> {
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