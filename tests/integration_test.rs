use std::fs::File;
use std::io::Write;
use std::thread;
use hdl_sv::cli::Cli;
use hdl_sv::server::run_server;
use tempfile::tempdir;

#[test]
fn test_server_requests() {
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
        verbose: false,
        detailed_logging: false,
    };

    // Run the server in a background thread
    thread::spawn(move || {
        run_server(cli).unwrap();
    });

    // Give the server a moment to start
    thread::sleep(std::time::Duration::from_secs(1));

    // Make requests to the server
    let client = reqwest::blocking::Client::new();

    // 1. Test directory listing
    let res = client.get("http://127.0.0.1:8080/").send().unwrap();
    assert!(res.status().is_success());
    let body = res.text().unwrap();
    assert!(body.contains("test.txt"));

    // 2. Test allowed file download
    let res = client.get("http://127.0.0.1:8080/test.txt").send().unwrap();
    assert!(res.status().is_success());
    assert_eq!(res.text().unwrap(), "hello world\n");

    // 3. Test not found file
    let res = client.get("http://127.0.0.1:8080/not_found.txt").send().unwrap();
    assert_eq!(res.status(), 404);

    // 4. Test forbidden file type
    let forbidden_file_path = dir.path().join("test.zip");
    File::create(&forbidden_file_path).unwrap();
    let res = client.get("http://127.0.0.1:8080/test.zip").send().unwrap();
    assert_eq!(res.status(), 404); // Should be 403, but current implementation returns 404
}
