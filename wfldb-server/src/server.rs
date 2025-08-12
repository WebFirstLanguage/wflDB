//! HTTP/2 server implementation

use hyper::server::conn::http2;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use http_body_util::Full;
use tokio::net::{TcpListener, TcpStream};
use std::net::SocketAddr;
use tracing::{error, info, debug};
use wfldb_engine::StorageEngine;
use crate::handlers::handle_request;

pub struct WflDBServer {
    storage: StorageEngine,
}

impl WflDBServer {
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

/// Simple HTTP response builder
pub fn simple_response(
    status: hyper::StatusCode,
    body: impl Into<String>,
) -> Result<Response<Full<bytes::Bytes>>, hyper::Error> {
    Ok(Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .header("server", "wfldb/0.1.0")
        .body(Full::new(bytes::Bytes::from(body.into())))
        .unwrap())
}

/// Create streaming response for large objects
pub fn streaming_response(
    data: Vec<u8>,
) -> Result<Response<Full<bytes::Bytes>>, hyper::Error> {
    Ok(Response::builder()
        .status(hyper::StatusCode::OK)
        .header("content-type", "application/octet-stream")
        .header("content-length", data.len().to_string())
        .header("server", "wfldb/0.1.0")
        .body(Full::new(bytes::Bytes::from(data)))
        .unwrap())
}