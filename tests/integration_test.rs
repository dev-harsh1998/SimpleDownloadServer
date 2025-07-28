use hdl_sv::cli::Cli;
use hdl_sv::server::run_server;
use std::fs::File;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use tempfile::tempdir;

struct TestServer {
    addr: SocketAddr,
    shutdown_tx: mpsc::Sender<()>,
    handle: Option<JoinHandle<()>>,
    _temp_dir: tempfile::TempDir,
}

fn setup_test_server(username: Option<String>, password: Option<String>) -> TestServer {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "hello world").unwrap();

    let cli = Cli {
        directory: dir.path().to_path_buf(),
        listen: "127.0.0.1".to_string(),
        port: 0, // Use port 0 to let the OS pick a free port
        allowed_extensions: "*.txt".to_string(),
        threads: 2,
        chunk_size: 1024,
        verbose: true,
        detailed_logging: true,
        username,
        password,
    };

    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let (addr_tx, addr_rx) = mpsc::channel();

    let server_handle = thread::spawn(move || {
        if let Err(e) = run_server(cli, Some(shutdown_rx), Some(addr_tx)) {
            eprintln!("Server thread failed: {e}");
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

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            self.shutdown_tx.send(()).ok(); // Use ok() to avoid panic on shutdown
            handle.join().unwrap();
        }
    }
}

#[test]
fn test_server_requests() {
    let server = setup_test_server(None, None);
    let client = reqwest::blocking::Client::new();

    // 1. Test directory listing
    let res = client
        .get(format!("http://{}/", server.addr))
        .send()
        .unwrap();
    assert!(res.status().is_success());
    let body = res.text().unwrap();
    assert!(body.contains("test.txt"));

    // 2. Test allowed file download
    let res = client
        .get(format!("http://{}/test.txt", server.addr))
        .send()
        .unwrap();
    assert!(res.status().is_success());
    assert_eq!(res.text().unwrap(), "hello world\n");

    // 3. Test not found file
    let res = client
        .get(format!("http://{}/not_found.txt", server.addr))
        .send()
        .unwrap();
    assert_eq!(res.status(), 404);

    // 4. Test forbidden file type
    let forbidden_file_path = server._temp_dir.path().join("test.zip");
    File::create(&forbidden_file_path).unwrap();
    let res = client
        .get(format!("http://{}/test.zip", server.addr))
        .send()
        .unwrap();
    assert_eq!(res.status(), 403);
}

#[test]
fn test_error_image_response() {
    let server = setup_test_server(None, None);
    let client = reqwest::blocking::Client::new();

    let res = client
        .get(format!("http://{}/not_found.txt", server.addr))
        .send()
        .unwrap();
    assert_eq!(res.status(), 404);
    assert_eq!(res.headers()["content-type"], "image/png");
}

#[test]
fn test_empty_request() {
    let server = setup_test_server(None, None);
    let _ = std::net::TcpStream::connect(server.addr).unwrap();
    // The connection is immediately closed here when the stream goes out of scope.
    // The server should handle this gracefully without panicking.
}

#[test]
fn test_authentication() {
    let server = setup_test_server(Some("testuser".to_string()), Some("testpass".to_string()));
    let client = reqwest::blocking::Client::new();

    // 1. Test without credentials
    let res = client
        .get(format!("http://{}/", server.addr))
        .send()
        .unwrap();
    assert_eq!(res.status(), 401);

    // 2. Test with wrong credentials
    let res = client
        .get(format!("http://{}/", server.addr))
        .basic_auth("wronguser", Some("wrongpass"))
        .send()
        .unwrap();
    assert_eq!(res.status(), 401);
}
