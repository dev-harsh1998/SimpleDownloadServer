use crate::utils::{get_request_path, percent_encode_path};
use crate::fs::{generate_directory_listing, generate_directory_row_html};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_percent_encode_path() {
    let path = Path::new("a b/c d");
    assert_eq!(percent_encode_path(path), "a%20b/c%20d");
}

#[test]
fn test_get_request_path() {
    assert_eq!(get_request_path("GET /path/to/file HTTP/1.1"), "path/to/file");
    assert_eq!(get_request_path("GET / HTTP/1.1"), "/");
    assert_eq!(get_request_path("GET /a%20b HTTP/1.1"), "a%20b");
}

#[test]
fn test_generate_directory_row_html() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let mut file = File::create(&file_path).unwrap();
    writeln!(file, "hello").unwrap();

    let row_html = generate_directory_row_html(&file_path, "TEST").unwrap();
    assert!(row_html.contains("test.txt"));
    assert!(row_html.contains("6 B")); // "hello" + newline
}

#[test]
fn test_generate_directory_listing() {
    let dir = tempdir().unwrap();
    File::create(dir.path().join("file1.txt")).unwrap();
    fs::create_dir(dir.path().join("subdir")).unwrap();

    let html = generate_directory_listing(&dir.path().to_path_buf(), "TEST").unwrap();
    assert!(html.contains("file1.txt"));
    assert!(html.contains("subdir"));
}
