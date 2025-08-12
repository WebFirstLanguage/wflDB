//! Wire format utilities

use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use crate::{WireFrame, RequestMessage, ResponseMessage};

/// High-level wire protocol client
pub struct WireClient {
    stream: TcpStream,
}

impl WireClient {
    pub fn connect(addr: &str) -> io::Result<Self> {
        let stream = TcpStream::connect(addr)?;
        Ok(WireClient { stream })
    }
    
    /// Send request and get response
    pub fn send_request(&mut self, request: RequestMessage, body: Vec<u8>) -> io::Result<(ResponseMessage, Vec<u8>)> {
        // Create wire frame
        let header_bytes = request.to_bytes();
        let frame = WireFrame::new(header_bytes, body);
        
        // Send frame
        let frame_bytes = frame.to_bytes();
        self.stream.write_all(&frame_bytes)?;
        self.stream.flush()?;
        
        // Read response (simplified - in real implementation would parse properly)
        let mut reader = BufReader::new(&mut self.stream);
        let mut response_line = String::new();
        reader.read_line(&mut response_line)?;
        
        // For spike, return dummy response
        let response = ResponseMessage::ok(request.request_id.clone());
        Ok((response, Vec::new()))
    }
}

/// Wire format utilities
pub struct WireUtils;

impl WireUtils {
    /// Calculate frame overhead for given header and body sizes
    pub fn frame_overhead(header_size: usize, body_size: usize) -> usize {
        4 + header_size // 4 bytes for header length + header size
        // Body is direct, no additional overhead
    }
    
    /// Estimate optimal chunk size for large objects
    pub fn optimal_chunk_size(total_size: usize, max_chunks: usize) -> usize {
        let chunk_size = (total_size + max_chunks - 1) / max_chunks; // Round up division
        
        // Align to 4KB boundaries for better I/O performance
        const ALIGNMENT: usize = 4096;
        ((chunk_size + ALIGNMENT - 1) / ALIGNMENT) * ALIGNMENT
    }
    
    /// Validate wire format constraints
    pub fn validate_sizes(header_size: usize, body_size: usize) -> Result<(), String> {
        if header_size > crate::protocol::MAX_HEADER_SIZE {
            return Err(format!("Header too large: {}", header_size));
        }
        
        if body_size > crate::protocol::MAX_SMALL_OBJECT_SIZE {
            return Err(format!("Body too large for single frame: {}", body_size));
        }
        
        Ok(())
    }
}

/// Performance monitoring for wire operations
#[derive(Debug, Default)]
pub struct WireMetrics {
    pub frames_sent: u64,
    pub frames_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub parse_time_us: u64,
    pub serialize_time_us: u64,
}

impl WireMetrics {
    pub fn new() -> Self {
        Default::default()
    }
    
    pub fn record_frame_sent(&mut self, size: usize) {
        self.frames_sent += 1;
        self.bytes_sent += size as u64;
    }
    
    pub fn record_frame_received(&mut self, size: usize) {
        self.frames_received += 1;
        self.bytes_received += size as u64;
    }
    
    pub fn record_parse_time(&mut self, microseconds: u64) {
        self.parse_time_us += microseconds;
    }
    
    pub fn record_serialize_time(&mut self, microseconds: u64) {
        self.serialize_time_us += microseconds;
    }
    
    pub fn avg_parse_time_us(&self) -> f64 {
        if self.frames_received == 0 {
            0.0
        } else {
            self.parse_time_us as f64 / self.frames_received as f64
        }
    }
    
    pub fn avg_serialize_time_us(&self) -> f64 {
        if self.frames_sent == 0 {
            0.0
        } else {
            self.serialize_time_us as f64 / self.frames_sent as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_wire_utils() {
        let header_size = 256;
        let body_size = 1024;
        
        let overhead = WireUtils::frame_overhead(header_size, body_size);
        assert_eq!(overhead, 4 + header_size);
        
        assert!(WireUtils::validate_sizes(header_size, body_size).is_ok());
        
        let chunk_size = WireUtils::optimal_chunk_size(1_000_000, 10);
        assert!(chunk_size >= 100_000);
        assert_eq!(chunk_size % 4096, 0); // Should be aligned
    }
    
    #[test]
    fn test_wire_metrics() {
        let mut metrics = WireMetrics::new();
        
        metrics.record_frame_sent(1024);
        metrics.record_frame_received(2048);
        metrics.record_parse_time(100);
        metrics.record_serialize_time(50);
        
        assert_eq!(metrics.frames_sent, 1);
        assert_eq!(metrics.frames_received, 1);
        assert_eq!(metrics.bytes_sent, 1024);
        assert_eq!(metrics.bytes_received, 2048);
        assert_eq!(metrics.avg_parse_time_us(), 100.0);
        assert_eq!(metrics.avg_serialize_time_us(), 50.0);
    }
}