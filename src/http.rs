use crate::error::AppError;
use crate::fs::generate_directory_listing;
use crate::response::send_response;
use crate::utils::get_request_path;
use glob::Pattern;
use log::{debug, info, warn};
use std::collections::HashMap;
use std::fs::File;
use std::io::{prelude::*, BufReader, ErrorKind, Read, Seek, SeekFrom};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Handles a single client connection.
pub fn handle_client(
    mut stream: TcpStream,
    file_directory: &Arc<Mutex<PathBuf>>,
    download_extensions: &Arc<Vec<Pattern>>,
    chunk_size: usize,
    log_prefix: &str,
    base_dir: &Arc<PathBuf>,
) -> Result<(), AppError> {
    let reader = BufReader::new(&stream);
    let mut lines_iter = reader.lines();

    let request_line = match lines_iter.next() {
        Some(Ok(line)) => line,
        Some(Err(e)) => return Err(AppError::Io(e)),
        None => return Err(AppError::BadRequest),
    };

    debug!("{} Request line: {}", log_prefix, request_line);

    let request_path_str = get_request_path(&request_line);
    let request_path = PathBuf::from(request_path_str.strip_prefix('/').unwrap_or(request_path_str));

    let mut headers_map = HashMap::new();
    for line in lines_iter {
        let line = line?;
        if line.is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(": ") {
            headers_map.insert(key.to_string(), value.to_string());
        }
    }

    let full_path = file_directory.lock().unwrap().join(&request_path);
    let canonical_path = full_path.canonicalize()?;

    if !canonical_path.starts_with(base_dir.as_ref()) {
        warn!(
            "{} Potential path traversal attempt: '{}'",
            log_prefix,
            request_path.display()
        );
        return Err(AppError::Forbidden);
    }

    if canonical_path.is_dir() {
        serve_directory(&mut stream, &canonical_path, log_prefix)?;
    } else if download_extensions
        .iter()
        .any(|pattern| pattern.matches_path(&request_path))
    {
        serve_file(
            &mut stream,
            &canonical_path,
            headers_map,
            chunk_size,
            log_prefix,
        )?;
    } else {
        warn!(
            "{} File extension not allowed or file not found for path: '{}'",
            log_prefix,
            request_path.display()
        );
        return Err(AppError::NotFound);
    }

    Ok(())
}

/// Serves a file to the client.
fn serve_file(
    stream: &mut TcpStream,
    path: &Path,
    headers: HashMap<String, String>,
    chunk_size: usize,
    log_prefix: &str,
) -> Result<(), AppError> {
    info!("{} serve_file started for: '{}'", log_prefix, path.display());
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == ErrorKind::NotFound => return Err(AppError::NotFound),
        Err(e) => return Err(AppError::Io(e)),
    };

    let file_size = file.metadata()?.len();
    let filename = path.file_name().unwrap().to_string_lossy();

    let (mut start_byte, mut end_byte) = (0, file_size - 1);
    let (status_code, status_text) = if let Some(range_header) = headers.get("Range") {
        if let Some(range) = parse_range_header(range_header, file_size) {
            start_byte = range.0;
            end_byte = range.1;
            (206, "Partial Content")
        } else {
            return Err(AppError::BadRequest);
        }
    } else {
        (200, "OK")
    };

    let content_length = end_byte - start_byte + 1;
    let mut response = format!(
        "HTTP/1.1 {} {}\r\nContent-Disposition: attachment; filename=\"{}\"\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nAccept-Ranges: bytes\r\n",
        status_code, status_text, filename, content_length
    );
    if status_code == 206 {
        response.push_str(&format!(
            "Content-Range: bytes {}-{}/{}\r\n",
            start_byte, end_byte, file_size
        ));
    }
    response.push_str("\r\n");

    stream.write_all(response.as_bytes())?;
    file.seek(SeekFrom::Start(start_byte))?;

    let mut bytes_remaining = content_length;
    let mut buffer = vec![0; chunk_size];
    while bytes_remaining > 0 {
        let to_read = std::cmp::min(bytes_remaining as usize, chunk_size);
        let bytes_read = file.read(&mut buffer[..to_read])?;
        if bytes_read == 0 {
            break;
        }
        stream.write_all(&buffer[..bytes_read])?;
        bytes_remaining -= bytes_read as u64;
    }

    info!("{} serve_file finished for: '{}'", log_prefix, path.display());
    Ok(())
}

/// Serves a directory listing as an HTML page.
fn serve_directory(
    stream: &mut TcpStream,
    path: &Path,
    log_prefix: &str,
) -> Result<(), AppError> {
    info!("{} serve_directory started for: '{}'", log_prefix, path.display());
    let html = generate_directory_listing(path, log_prefix)?;
    send_response(stream, 200, "OK", &html, log_prefix)?;
    info!("{} serve_directory finished for: '{}'", log_prefix, path.display());
    Ok(())
}

fn parse_range_header(header: &str, file_size: u64) -> Option<(u64, u64)> {
    let header = header.strip_prefix("bytes=")?;
    let mut parts = header.split('-');
    let start = parts.next()?.parse::<u64>().ok()?;
    let end = parts
        .next()
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(file_size - 1);

    if start > end || end >= file_size {
        None
    } else {
        Some((start, end))
    }
}
