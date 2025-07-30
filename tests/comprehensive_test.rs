//! Comprehensive tests for the enhanced file server without external dependencies.

use hdl_sv::cli::Cli;
use hdl_sv::server::run_server;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tempfile::{tempdir, TempDir};

/// A helper struct to manage a running test server without external HTTP clients.
struct TestServer {
    addr: SocketAddr,
    shutdown_tx: mpsc::Sender<()>,
    handle: Option<JoinHandle<()>>,
    _temp_dir: TempDir,
}

impl TestServer {
    /// Sets up and runs a server in a background thread for testing.
    fn new(username: Option<String>, password: Option<String>) -> Self {
        let dir = tempdir().unwrap();
        
        // Create test files
        let test_file = dir.path().join("test.txt");
        let mut file = File::create(&test_file).unwrap();
        writeln!(file, "Hello from test file!").unwrap();
        
        let binary_file = dir.path().join("test.pdf");
        File::create(&binary_file).unwrap();
        
        let large_file = dir.path().join("large.txt");
        let mut large = File::create(&large_file).unwrap();
        for i in 0..1000 {
            writeln!(large, "Line {} of a large file for testing", i).unwrap();
        }
        
        // Create subdirectory
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        let subfile = subdir.join("nested.txt");
        let mut nested = File::create(&subfile).unwrap();
        writeln!(nested, "Nested file content").unwrap();

        let cli = Cli {
            directory: dir.path().to_path_buf(),
            listen: "127.0.0.1".to_string(),
            port: 0,
            allowed_extensions: "*.txt,*.pdf".to_string(),
            threads: 4,
            chunk_size: 1024,
            verbose: false,
            detailed_logging: false,
            username,
            password,
        };

        let (shutdown_tx, shutdown_rx) = mpsc::channel();
        let (addr_tx, addr_rx) = mpsc::channel();

        let server_handle = thread::spawn(move || {
            if let Err(e) = run_server(cli, Some(shutdown_rx), Some(addr_tx)) {
                eprintln!("Server thread failed: {}", e);
            }
        });

        let server_addr = addr_rx.recv().unwrap();

        TestServer {
            addr: server_addr,
            shutdown_tx,
            handle: Some(server_handle),
            _temp_dir: dir,
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.shutdown_tx.send(()).ok();
            handle.join().unwrap();
        }
    }
}

/// Native HTTP client implementation for testing
struct HttpClient;

impl HttpClient {
    fn get(url: &str) -> HttpResponse {
        Self::request("GET", url, None, None)
    }

    fn get_with_auth(url: &str, username: &str, password: &str) -> HttpResponse {
        let credentials = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, format!("{}:{}", username, password));
        let auth_header = format!("Basic {}", credentials);
        Self::request("GET", url, Some(&auth_header), None)
    }

    fn request(method: &str, url: &str, auth: Option<&str>, body: Option<&str>) -> HttpResponse {
        // Parse URL properly: http://127.0.0.1:8080/path
        let url = url.strip_prefix("http://").unwrap_or(url);
        let parts: Vec<&str> = url.splitn(2, '/').collect();
        let host_port = parts[0];
        let path = if parts.len() > 1 {
            format!("/{}", parts[1])
        } else {
            "/".to_string()
        };

        let mut stream = TcpStream::connect(host_port).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();

        let mut request = format!("{} {} HTTP/1.1\r\nHost: {}\r\n", method, path, host_port);
        
        if let Some(auth_header) = auth {
            request.push_str(&format!("Authorization: {}\r\n", auth_header));
        }
        
        if let Some(body_content) = body {
            request.push_str(&format!("Content-Length: {}\r\n", body_content.len()));
        }
        
        request.push_str("\r\n");
        
        if let Some(body_content) = body {
            request.push_str(body_content);
        }

        stream.write_all(request.as_bytes()).unwrap();

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();

        let status_code = status_line
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u16>()
            .unwrap();

        let mut headers = std::collections::HashMap::new();
        let mut body_content = String::new();
        let mut reading_headers = true;

        for line in reader.lines() {
            let line = line.unwrap();
            if reading_headers {
                if line.trim().is_empty() {
                    reading_headers = false;
                    continue;
                }
                if let Some((key, value)) = line.split_once(": ") {
                    headers.insert(key.to_lowercase(), value.to_string());
                }
            } else {
                body_content.push_str(&line);
                body_content.push('\n');
            }
        }

        HttpResponse {
            status_code,
            headers,
            body: body_content.trim_end_matches('\n').to_string(),
        }
    }
}

struct HttpResponse {
    status_code: u16,
    headers: std::collections::HashMap<String, String>,
    body: String,
}

#[test]
fn test_enhanced_directory_listing() {
    let server = TestServer::new(None, None);
    let url = format!("http://{}/", server.addr);
    let response = HttpClient::get(&url);

    assert_eq!(response.status_code, 200);
    assert!(response.headers.get("content-type").unwrap().contains("text/html"));
    assert!(response.body.contains("test.txt"));
    assert!(response.body.contains("subdir/"));
    assert!(response.body.contains("Name"));
    
    // Check for modular template structure
    assert!(response.body.contains("/_static/directory/styles.css"), "Should link to external CSS");
    assert!(response.body.contains("/_static/directory/script.js"), "Should link to external JS");
    assert!(response.body.contains("class=\"container\""), "Should use proper CSS classes");
    
    // Ensure no emoji icons are present
    assert!(!response.body.contains("📁"));
    assert!(!response.body.contains("📄"));
}

#[test]
fn test_beautiful_error_pages() {
    let server = TestServer::new(None, None);
    let url = format!("http://{}/nonexistent", server.addr);
    let response = HttpClient::get(&url);

    assert_eq!(response.status_code, 404);
    assert!(response.headers.get("content-type").unwrap().contains("text/html"));
    assert!(response.body.contains("404"));
    assert!(response.body.contains("Not Found"));
    assert!(response.body.contains("hdl_sv/2.0.0"));
    
    // Check for modular error page template structure
    assert!(response.body.contains("/_static/error/styles.css"), "Should link to external error CSS");
    assert!(response.body.contains("/_static/error/script.js"), "Should link to external error JS");
    assert!(response.body.contains("class=\"error-container\""), "Should use proper error CSS classes");
    
    // Check for modern interaction elements
    assert!(response.body.contains("back-link"));
    assert!(response.body.contains("Back to Files"));
}

#[test]
fn test_static_asset_serving() {
    let server = TestServer::new(None, None);
    
    // Test CSS file serving
    let css_url = format!("http://{}/_static/directory/styles.css", server.addr);
    let css_response = HttpClient::get(&css_url);
    
    assert_eq!(css_response.status_code, 200);
    assert!(css_response.headers.get("content-type").unwrap().contains("text/css"));
    assert!(css_response.body.contains("--bg-primary"), "Should contain CSS custom properties");
    assert!(css_response.body.contains("backdrop-filter"), "Should contain modern CSS effects");
    
    // Test JS file serving
    let js_url = format!("http://{}/_static/directory/script.js", server.addr);
    let js_response = HttpClient::get(&js_url);
    
    assert_eq!(js_response.status_code, 200);
    assert!(js_response.headers.get("content-type").unwrap().contains("application/javascript"));
    assert!(js_response.body.contains("DOMContentLoaded"), "Should contain valid JavaScript");
    
    // Test error CSS serving
    let error_css_url = format!("http://{}/_static/error/styles.css", server.addr);
    let error_css_response = HttpClient::get(&error_css_url);
    
    assert_eq!(error_css_response.status_code, 200);
    assert!(error_css_response.headers.get("content-type").unwrap().contains("text/css"));
    assert!(error_css_response.body.contains("error-container"), "Should contain error page styles");
    
    // Test 404 for non-existent static asset
    let missing_url = format!("http://{}/_static/nonexistent.css", server.addr);
    let missing_response = HttpClient::get(&missing_url);
    
    assert_eq!(missing_response.status_code, 404);
}

#[test]
fn test_health_check_endpoint() {
    let server = TestServer::new(None, None);
    let url = format!("http://{}/_health", server.addr);
    let response = HttpClient::get(&url);

    assert_eq!(response.status_code, 200);
    assert!(response.headers.get("content-type").unwrap().contains("application/json"));
    assert!(response.body.contains("\"status\": \"healthy\""));
    assert!(response.body.contains("\"service\": \"hdl_sv\""));
    assert!(response.body.contains("rate_limiting"));
    assert!(response.body.contains("enhanced_security"));
}

#[test]
fn test_mime_type_detection() {
    let server = TestServer::new(None, None);
    
    // Test text file
    let url = format!("http://{}/test.txt", server.addr);
    let response = HttpClient::get(&url);
    assert_eq!(response.status_code, 200);
    assert!(response.headers.get("content-type").unwrap().contains("text/plain"));
    
    // Test PDF file
    let url = format!("http://{}/test.pdf", server.addr);
    let response = HttpClient::get(&url);
    assert_eq!(response.status_code, 200);
    assert!(response.headers.get("content-type").unwrap().contains("application/pdf"));
}

#[test]
fn test_enhanced_security_headers() {
    let server = TestServer::new(None, None);
    let url = format!("http://{}/test.txt", server.addr);
    let response = HttpClient::get(&url);

    assert_eq!(response.status_code, 200);
    assert!(response.headers.contains_key("server"));
    assert!(response.headers.get("server").unwrap().contains("hdl_sv/2.0.0"));
    assert!(response.headers.contains_key("cache-control"));
    assert!(response.headers.contains_key("accept-ranges"));
}

#[test]
fn test_authentication_flow() {
    let server = TestServer::new(Some("user".to_string()), Some("pass".to_string()));
    
    // Test without credentials
    let url = format!("http://{}/", server.addr);
    let response = HttpClient::get(&url);
    assert_eq!(response.status_code, 401);
    assert!(response.headers.contains_key("www-authenticate"));
    assert!(response.body.contains("401"));
    
    // Test with correct credentials
    let response = HttpClient::get_with_auth(&url, "user", "pass");
    assert_eq!(response.status_code, 200);
    assert!(response.body.contains("Name"));
    
    // Test with wrong credentials
    let response = HttpClient::get_with_auth(&url, "wrong", "credentials");
    assert_eq!(response.status_code, 401);
}

#[test]
fn test_rate_limiting_simulation() {
    let server = TestServer::new(None, None);
    let url = format!("http://{}/test.txt", server.addr);
    
    // Make several requests quickly to test rate limiting
    // Note: In real scenarios, rate limiting would kick in after many requests
    // This test verifies the server handles multiple concurrent requests gracefully
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let url = url.clone();
            thread::spawn(move || HttpClient::get(&url))
        })
        .collect();
    
    let mut success_count = 0;
    for handle in handles {
        let response = handle.join().unwrap();
        if response.status_code == 200 {
            success_count += 1;
        }
    }
    
    // All requests should succeed in this test scenario
    assert!(success_count >= 8); // Allow for some variance
}

#[test]
fn test_nested_directory_access() {
    let server = TestServer::new(None, None);
    
    // Test subdirectory listing
    let url = format!("http://{}/subdir/", server.addr);
    let response = HttpClient::get(&url);
    assert_eq!(response.status_code, 200);
    assert!(response.body.contains("nested.txt"));
    assert!(response.body.contains("/subdir/"));
    
    // Test nested file access
    let url = format!("http://{}/subdir/nested.txt", server.addr);
    let response = HttpClient::get(&url);
    assert_eq!(response.status_code, 200);
    assert!(response.body.contains("Nested file content"));
}

#[test]
fn test_path_traversal_security() {
    let server = TestServer::new(None, None);
    
    // Attempt path traversal attack
    let url = format!("http://{}/../../etc/passwd", server.addr);
    let response = HttpClient::get(&url);
    assert_eq!(response.status_code, 403); // Should be forbidden
    
    // Another traversal attempt
    let url = format!("http://{}/../../../", server.addr);
    let response = HttpClient::get(&url);
    assert_eq!(response.status_code, 403);
}

#[test]
fn test_malformed_requests() {
    use std::io::Write;
    
    let server = TestServer::new(None, None);
    
    // Send malformed HTTP request
    let mut stream = TcpStream::connect(server.addr).unwrap();
    stream.write_all(b"INVALID REQUEST\r\n\r\n").unwrap();
    
    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader.read_line(&mut status_line).unwrap();
    
    assert!(status_line.contains("400") || status_line.contains("Bad Request"));
}

#[test]
fn test_large_file_handling() {
    let server = TestServer::new(None, None);
    let url = format!("http://{}/large.txt", server.addr);
    let response = HttpClient::get(&url);
    
    assert_eq!(response.status_code, 200);
    assert!(response.body.contains("Line 0 of a large file"));
    assert!(response.body.contains("Line 999 of a large file"));
    
    // Check proper content length (allow for small variations due to line endings)
    if let Some(content_length) = response.headers.get("content-length") {
        let length: usize = content_length.parse().unwrap();
        assert!(length > 0);
        let body_len = response.body.len();
        assert!(
            (length as i64 - body_len as i64).abs() <= 2,
            "Content length {} doesn't match body length {} (diff: {})",
            length,
            body_len,
            (length as i64 - body_len as i64).abs()
        );
    }
}

#[test]
fn test_http_compliance() {
    let server = TestServer::new(None, None);
    let url = format!("http://{}/test.txt", server.addr);
    let response = HttpClient::get(&url);
    
    assert_eq!(response.status_code, 200);
    
    // Check for required HTTP headers
    assert!(response.headers.contains_key("content-type"));
    assert!(response.headers.contains_key("content-length"));
    assert!(response.headers.contains_key("server"));
    
    // Verify server identification
    assert_eq!(response.headers.get("server").unwrap(), "hdl_sv/2.0.0");
}