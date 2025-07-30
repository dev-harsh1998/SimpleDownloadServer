use crate::error::AppError;
use crate::fs::generate_directory_listing;
use crate::response::send_response;
use crate::utils::get_request_path;
use base64::engine::general_purpose;
use base64::Engine;
use glob::Pattern;
use log::{error, info, warn};
use std::collections::HashMap;
use std::fs::File;
use std::io::{prelude::*, BufReader, ErrorKind, Read, Seek, SeekFrom};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[allow(clippy::too_many_arguments)]
pub fn handle_client(
    mut stream: TcpStream,
    file_directory: &Arc<Mutex<PathBuf>>,
    download_extensions: &Arc<Vec<Pattern>>,
    chunk_size: usize,
    log_prefix: &str,
    base_dir: &Arc<PathBuf>,
    username: &Arc<Option<String>>,
    password: &Arc<Option<String>>,
) {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    if let Err(e) = handle_request(
        &mut stream,
        file_directory,
        download_extensions,
        chunk_size,
        log_prefix,
        base_dir,
        username,
        password,
    ) {
        send_error_response(&mut stream, e, log_prefix);
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_request(
    stream: &mut TcpStream,
    file_directory: &Arc<Mutex<PathBuf>>,
    download_extensions: &Arc<Vec<Pattern>>,
    chunk_size: usize,
    log_prefix: &str,
    base_dir: &Arc<PathBuf>,
    username: &Arc<Option<String>>,
    password: &Arc<Option<String>>,
) -> Result<(), AppError> {
    let reader = BufReader::new(&*stream);
    let mut lines_iter = reader.lines();

    let request_line = match lines_iter.next() {
        Some(Ok(line)) => line,
        Some(Err(e)) => {
            return Err(AppError::Io(e));
        }
        None => {
            return Err(AppError::BadRequest);
        }
    };

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
    if let (Some(username), Some(password)) =
        (username.as_ref().as_ref(), password.as_ref().as_ref())
    {
        if !authenticate(&headers_map, username, password)? {
            return Err(AppError::Unauthorized);
        }
    }

    let request_path_str = get_request_path(&request_line);
    let request_path = PathBuf::from(
        request_path_str
            .strip_prefix('/')
            .unwrap_or(request_path_str),
    );

    let full_path = file_directory.lock().unwrap().join(&request_path);
    let canonical_path = match full_path.canonicalize() {
        Ok(path) => path,
        Err(e) if e.kind() == ErrorKind::NotFound => return Err(AppError::NotFound),
        Err(e) => return Err(AppError::Io(e)),
    };

    if !canonical_path.starts_with(base_dir.as_ref()) {
        warn!(
            "{} Potential path traversal attempt: '{}'",
            log_prefix,
            request_path.display()
        );
        return Err(AppError::Forbidden);
    }

    if canonical_path.is_dir() {
        serve_directory(stream, &canonical_path, log_prefix)?;
    } else if download_extensions
        .iter()
        .any(|pattern| pattern.matches_path(&request_path))
    {
        serve_file(stream, &canonical_path, headers_map, chunk_size, log_prefix)?;
    } else {
        warn!(
            "{} File extension not allowed for path: '{}'",
            log_prefix,
            request_path.display()
        );
        return Err(AppError::Forbidden);
    }

    Ok(())
}

fn send_error_response(stream: &mut TcpStream, err: AppError, log_prefix: &str) {
    let (status_code, status_text, body) = match err {
        AppError::NotFound => (404, "Not Found", "The requested resource was not found."),
        AppError::Forbidden => (
            403,
            "Forbidden",
            "You do not have permission to access this resource.",
        ),
        AppError::BadRequest => (
            400,
            "Bad Request",
            "The server could not understand the request.",
        ),
        AppError::Unauthorized => (401, "Unauthorized", "Authentication required."),
        AppError::InternalServerError(ref msg) => (500, "Internal Server Error", msg.as_str()),
        AppError::Io(ref e)
            if e.kind() == ErrorKind::ConnectionReset
                || e.kind() == ErrorKind::BrokenPipe
                || e.kind() == ErrorKind::WouldBlock =>
        {
            warn!("{log_prefix} Connection error when sending response: {e}");
            return;
        }
        _ => (
            500,
            "Internal Server Error",
            "An unexpected error occurred.",
        ),
    };

    error!("{log_prefix} Responding with error {status_code}: {status_text}");

    let mut headers = HashMap::new();
    if status_code == 401 {
        headers.insert("WWW-Authenticate", "Basic realm=\"Restricted\"");
    }

    if let Err(e) = send_response(stream, status_code, status_text, body, log_prefix) {
        error!("{log_prefix} Failed to send error response: {e}");
    }
}

fn authenticate(
    headers: &HashMap<String, String>,
    username: &str,
    password: &str,
) -> Result<bool, AppError> {
    if let Some(auth_header) = headers.get("Authorization") {
        if let Some(credentials) = auth_header.strip_prefix("Basic ") {
            let decoded = match general_purpose::STANDARD.decode(credentials) {
                Ok(decoded) => decoded,
                Err(_) => return Ok(false),
            };
            let decoded_str = match String::from_utf8(decoded) {
                Ok(s) => s,
                Err(_) => return Ok(false),
            };
            let mut parts = decoded_str.splitn(2, ':');
            let provided_user = parts.next();
            let provided_pass = parts.next();

            if let (Some(user), Some(pass)) = (provided_user, provided_pass) {
                return Ok(user == username && pass == password);
            }
        }
    }
    Ok(false)
}

/// Serves a file to the client.
fn serve_file(
    stream: &mut TcpStream,
    path: &Path,
    headers: HashMap<String, String>,
    chunk_size: usize,
    log_prefix: &str,
) -> Result<(), AppError> {
    info!(
        "{} serve_file started for: '{}'",
        log_prefix,
        path.display()
    );
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
        "HTTP/1.1 {status_code} {status_text}\r\nContent-Disposition: attachment; filename=\"{filename}\"\r\nContent-Length: {content_length}\r\nContent-Type: application/octet-stream\r\nAccept-Ranges: bytes\r\n"
    );
    if status_code == 206 {
        response.push_str(&format!(
            "Content-Range: bytes {start_byte}-{end_byte}/{file_size}\r\n"
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

    info!(
        "{} serve_file finished for: '{}'",
        log_prefix,
        path.display()
    );
    Ok(())
}

/// Serves a directory listing as an HTML page.
fn serve_directory(stream: &mut TcpStream, path: &Path, log_prefix: &str) -> Result<(), AppError> {
    info!(
        "{} serve_directory started for: '{}'",
        log_prefix,
        path.display()
    );
    let html = generate_directory_listing(path, log_prefix)?;
    send_response(stream, 200, "OK", &html, log_prefix)?;
    info!(
        "{} serve_directory finished for: '{}'",
        log_prefix,
        path.display()
    );
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
