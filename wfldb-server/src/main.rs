//! wflDB server implementation - Phase 0 spike version

use clap::{Arg, Command};
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::{info, warn};
use wfldb_engine::StorageEngine;

mod simple_server;

use simple_server::SimpleServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let matches = Command::new("wfldb-server")
        .version("0.1.0")
        .about("High-performance permissioned key-object store")
        .arg(
            Arg::new("data-dir")
                .long("data-dir")
                .value_name("PATH")
                .help("Data directory path")
                .default_value("./data")
        )
        .arg(
            Arg::new("bind")
                .long("bind")
                .value_name("ADDR")
                .help("Bind address")
                .default_value("127.0.0.1:8080")
        )
        .get_matches();

    let data_dir: PathBuf = matches.get_one::<String>("data-dir")
        .unwrap()
        .parse()
        .expect("Invalid data directory path");
    
    let bind_addr: SocketAddr = matches.get_one::<String>("bind")
        .unwrap()
        .parse()
        .expect("Invalid bind address");

    info!("Starting wflDB server (Phase 0 Spike)");
    info!("Data directory: {}", data_dir.display());
    info!("Bind address: {}", bind_addr);

    // Create data directory if it doesn't exist
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)?;
        info!("Created data directory: {}", data_dir.display());
    }

    // Initialize storage engine
    let storage_engine = StorageEngine::new(&data_dir)
        .map_err(|e| format!("Failed to initialize storage engine: {}", e))?;

    info!("Storage engine initialized");

    // Create and start server
    let server = SimpleServer::new(storage_engine);
    
    match server.serve(bind_addr).await {
        Ok(_) => info!("Server shutdown gracefully"),
        Err(e) => {
            warn!("Server error: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}