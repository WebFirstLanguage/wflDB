//! Build script to generate FlatBuffers code from schemas

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=schemas/");

    let out_dir = env::var("OUT_DIR").unwrap();
    let schema_dir = Path::new("schemas");
    
    // Find flatbuffers compiler (flatc) - optional for spike phase
    match find_flatc() {
        Some(flatc) => {
            println!("Found flatc compiler: {}", flatc);
            
            // Generate Rust code from .fbs files
            if schema_dir.exists() {
                for entry in fs::read_dir(schema_dir).unwrap() {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    
                    if path.extension().and_then(|s| s.to_str()) == Some("fbs") {
                        println!("cargo:rerun-if-changed={}", path.display());
                        
                        let output = Command::new(&flatc)
                            .arg("--rust")
                            .arg("--gen-mutable") 
                            .arg("--gen-object-api")
                            .arg("-o")
                            .arg(&out_dir)
                            .arg(&path)
                            .output()
                            .expect("Failed to execute flatc");
                        
                        if !output.status.success() {
                            panic!(
                                "flatc failed for {}: {}",
                                path.display(),
                                String::from_utf8_lossy(&output.stderr)
                            );
                        }
                        
                        println!("Generated FlatBuffers code for {}", path.display());
                    }
                }
            }
        }
        None => {
            println!("cargo:warning=flatc compiler not found - using simplified wire format for spike phase");
            println!("cargo:warning=For production builds, install FlatBuffers compiler: https://google.github.io/flatbuffers/flatbuffers_guide_building.html");
        }
    }
    
    println!("cargo:rustc-env=FLATBUFFERS_OUT_DIR={}", out_dir);
}

fn find_flatc() -> Option<String> {
    // Try common locations
    let candidates = vec![
        "flatc",
        "flatc.exe", 
        "/usr/local/bin/flatc",
        "/usr/bin/flatc",
    ];
    
    for candidate in candidates {
        if Command::new(candidate)
            .arg("--version")
            .output()
            .is_ok()
        {
            return Some(candidate.to_string());
        }
    }
    
    // For development, we'll create a stub implementation
    None
}