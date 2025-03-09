/*
 * SPDX-License-Identifier: GPL-3.0-or-later
 * More licensing information can be found in the project LICENSE file
 * Author: Harshit Jain
 * Email: reach@harsh1998.dev
 */
use chrono::{DateTime, Local}; // Removed unused import TimeZone
use clap::Parser;
use glob::Pattern;
use humansize::{file_size_opts as options, FileSize};
use rust_embed::RustEmbed;
use std::fs::{self, File}; // Removed unused imports Metadata, ReadDir
use std::io::{prelude::*, Read, Seek, SeekFrom, ErrorKind}; // Removed unused import Error as IoError
use std::net::{TcpListener, TcpStream, SocketAddr, Shutdown};
use std::path::{Path, PathBuf, Component};
use std::sync::{Arc, Mutex}; // Removed unused import MutexGuard
use std::str::FromStr;
use std::time::SystemTime;
use threadpool::ThreadPool;
use std::thread;
use std::ffi::OsStr;

#[derive(RustEmbed)]
#[folder = "assets"]
struct Assets;

#[derive(Parser)]
#[command(
    author = "Harshit Jain",
    version = "1.5.0",
    long_about = "This is a simple configurable download server that serves files from a directory.
It can be used to share files with others or to download files from a remote server.
The server can be configured to serve only specific file extensions and can be run on a specific host and port.
if the requested path is a directory, the server will generate an HTML page with a list of files and subdirectories in the directory.
The server will respond with a 403 Forbidden error if the requested file extension is not allowed.
The server will respond with a 404 Not Found error if the requested file or directory does not exist.
The server will respond with a 400 Bad Request error if the request is invalid.
The server will only serve files from the specified directory and not from subdirectories.
Author: Harshit Jain
",
    about = "A simple configurable download server that serves files from a directory."
)]
struct Cli {
    /// Directory path to serve, mandatory
    #[arg(short, long, required = true)]
    directory: PathBuf,
    /// Host address to listen on (e.g., "127.0.0.1", "0.0.0.0")
    #[arg(short, long, default_value = "127.0.0.1")]
    listen: String,
    /// Port number to listen on
    #[arg(short, long, default_value_t = 8080)]
    port: u16,
    /// Allowed file extensions for download (comma-separated, supports wildcards)
    #[arg(short, long, default_value = "*.zip,*.txt")]
    allowed_extensions: String,
    /// Number of threads in the thread pool
    #[arg(short, long, default_value_t = 64)]
    threads: usize,
    /// Chunk size for reading files (in bytes)
    /// This is the size of the buffer used to read files in chunks
    #[arg(short, long, default_value_t = 1024)]
    chunk_size: usize,
}

fn main() {
    if let Err(e) = run_server() {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    }
}

fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let file_directory = Arc::new(Mutex::new(
        PathBuf::from(cli.directory.clone()).canonicalize().map_err(|e| format!("Failed to canonicalize directory path: {}", e))?
    ));

    // Validate directory existence and access
    if !is_directory(&file_directory).map_err(|e| format!("Error checking directory: {}", e))? {
        return Err(From::from(format!("Directory '{}' does not exist or is not accessible.", cli.directory.display())));
    }

    let allowed_extensions = Arc::new(
        cli.allowed_extensions
            .split(',')
            .map(|ext| Pattern::new(ext.trim()).map_err(|e| format!("Invalid extension pattern '{}': {}", ext.trim(), e)))
            .collect::<Result<Vec<Pattern>, String>>().map_err(|e| format!("Error parsing allowed extensions: {}", e))?
    );

    let listener = TcpListener::bind(format!("{}:{}", cli.listen, cli.port)).map_err(|e| format!("Failed to bind to address {}:{}: {}", cli.listen, cli.port, e))?;
    println!(
        "Listening on {}:{} for directory {} (allowed extensions: {:?})",
        cli.listen,
        cli.port,
        file_directory.lock().map_err(|_| "Failed to lock directory mutex".to_string())?.display(),
        allowed_extensions
    );

    let pool = ThreadPool::new(cli.threads);

    for stream_result in listener.incoming() {
        match stream_result {
            Ok(stream) => {
                let file_directory_arc = Arc::clone(&file_directory);
                let allowed_extensions_arc = Arc::clone(&allowed_extensions);
                let chunk_size = cli.chunk_size;
                pool.execute(move || {
                    if let Err(e) = handle_client(stream, &file_directory_arc, &allowed_extensions_arc, chunk_size) {
                        eprintln!("Error handling client: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }
    }
    Ok(())
}

fn is_directory(file_directory: &Arc<Mutex<PathBuf>>) -> Result<bool, String> {
    let dir_guard_result = file_directory.lock().map_err(|_| "Failed to lock directory mutex".to_string())?;
    Ok(Path::new(&*dir_guard_result).is_dir())
}


fn handle_client(
    mut stream: TcpStream,
    file_directory: &Arc<Mutex<PathBuf>>,
    download_extensions: &Arc<Vec<Pattern>>,
    chunk_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let peer_addr = stream.peer_addr().map_err(|e| format!("Failed to get peer address: {}", e))?;
    println!("[{:?}] handle_client started for client: {:?}", thread::current().id(), peer_addr);
    let mut buffer = [0; 1024];
    let request_line: String; // Declare request_line here without initial assignment
    let mut headers = Vec::new();

    // Read the request line
    let bytes_read_result = stream.read(&mut buffer);
    let bytes_read = match bytes_read_result {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("[{:?}] Error reading request from {}: {}", thread::current().id(), peer_addr, e);
            return Ok(()); // Don't propagate, just log and close connection
        }
    };

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let mut lines = request.lines();

    if let Some(line) = lines.next() {
        request_line = line.to_string(); // Assign value here
    } else {
        send_response(&mut stream, 400, "Bad Request", "Empty request").map_err(|e| format!("Failed to send response to {}: {}", peer_addr, e))?;
        return Ok(());
    }

    // Read the headers
    for line in lines {
        if line.is_empty() {
            break;
        }
        headers.push(line.to_string());
    }

    // Print the request line and headers (consider using a logger for more structured logging)
    println!("[{:?}] Request from {}: {}", thread::current().id(), peer_addr, request_line);
    for header in &headers {
        println!("[{:?}] Header from {}: {}", thread::current().id(), peer_addr, header);
    }

    let requested_path = request_line.split_whitespace().nth(1);

    let file_directory_guard_result = file_directory.lock().map_err(|_| "Failed to lock directory mutex".to_string());
    let file_directory_guard = match file_directory_guard_result {
        Ok(guard) => guard,
        Err(e) => {
            eprintln!("[{:?}] Error locking directory mutex for client {}: {}", thread::current().id(), peer_addr, e);
            send_response(&mut stream, 500, "Internal Server Error", "Failed to access server directory").map_err(|e| format!("Failed to send error response to {}: {}", peer_addr, e))?;
            return Ok(());
        }
    };

    let path_result: Result<PathBuf, String> = match requested_path {
        Some(path) if path.starts_with('/') => {
            Ok(file_directory_guard.join(path.trim_start_matches('/')))
        }
        _ => {
            send_response(&mut stream, 400, "Bad Request", "Invalid request path").map_err(|e| format!("Failed to send response to {}: {}", peer_addr, e))?;
            return Ok(());
        }
    };

    let path = match path_result {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[{:?}] Error processing path for client {}: {}", thread::current().id(), peer_addr, e);
            return Ok(());
        }
    };


    if !path.exists() {
        send_response(&mut stream, 404, "Not Found", "File or directory not found").map_err(|e| format!("Failed to send response to {}: {}", peer_addr, e))?;
        return Ok(());
    }

    // Security: Path traversal check
    if !path.starts_with(&*file_directory_guard) {
        send_response(&mut stream, 403, "Forbidden", "Access denied").map_err(|e| format!("Failed to send response to {}: {}", peer_addr, e))?;
        return Ok(());
    }

    let file_extension_allowed = if let Some(ext) = path.extension().and_then(OsStr::to_str) {
        download_extensions.iter().any(|pattern| pattern.matches(&format!(".{}", ext)))
    } else {
        // Allow files with no extension if no patterns are specified or if a pattern allows it
        download_extensions.is_empty() || download_extensions.iter().any(|pattern| pattern.matches(""))
    };

    if !path.is_dir() && file_extension_allowed {
        serve_file(&mut stream, &path, headers, chunk_size, peer_addr)?;
    } else if path.is_dir() {
        serve_directory(&mut stream, &path, peer_addr)?;
    } else {
        send_response(
            &mut stream,
            403,
            "Forbidden",
            "Only allowed files can be downloaded",
        ).map_err(|e| format!("Failed to send response to {}: {}", peer_addr, e))?;
    }
    println!("[{:?}] handle_client finished for client: {:?}", thread::current().id(), peer_addr);
    Ok(())
}

fn serve_file(stream: &mut TcpStream, path: &PathBuf, headers: Vec<String>, chunk_size: usize, peer_addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{:?}] serve_file started for: {:?} client: {:?}", thread::current().id(), path, peer_addr);
    println!("[{:?}] Opening file: {:?} for client: {:?}", thread::current().id(), path, peer_addr);
    let file_result = File::open(path);
    let mut file = match file_result {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[{:?}] Failed to open file {:?} for client {}: {}", thread::current().id(), path, peer_addr, e);
            send_response(stream, 500, "Internal Server Error", "Failed to open file").map_err(|e| format!("Failed to send error response to {}: {}", peer_addr, e))?;
            return Ok(());
        }
    };
    println!("[{:?}] File opened successfully: {:?} for client {:?}", thread::current().id(), path, peer_addr);

    let metadata_result = file.metadata();
    let file_size: u64 = match metadata_result {
        Ok(metadata) => metadata.len(),
        Err(e) => {
            eprintln!("[{:?}] Failed to get metadata for {:?} for client {}: {}", thread::current().id(), path, peer_addr, e);
            send_response(stream, 500, "Internal Server Error", "Failed to read file metadata").map_err(|e| format!("Failed to send error response to {}: {}", peer_addr, e))?;
            return Ok(());
        }
    };
    let filename_os = path.file_name().ok_or_else(|| format!("No filename for path {:?}", path))?;
    let filename = filename_os.to_string_lossy();

    let mut range_header = String::new();
    for header in headers {
        if header.starts_with("Range: bytes=") {
            range_header = header.trim_start_matches("Range: bytes=").to_string();
            break;
        }
    }

    let mut start_byte = 0;
    let mut end_byte = file_size - 1;

    let response: String;
    if !range_header.is_empty() {
        let range_parts: Vec<&str> = range_header.split('-').collect();
        if range_parts.len() == 2 {
            if let Ok(start) = u64::from_str(range_parts[0]) {
                start_byte = start;
            }
            if let Ok(end) = u64::from_str(range_parts[1]) {
                end_byte = end;
            }
        }
        // Send 206 Partial Content if Range header is present
        response = format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {}-{}/{}\r\nContent-Disposition: attachment; filename=\"{}\"\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n", // ADDED Content-Type header
            start_byte, end_byte, file_size, filename, end_byte - start_byte + 1
        );
    } else {
        // Send 200 OK if no Range header is present
        response = format!(
            "HTTP/1.1 200 OK\r\nContent-Disposition: attachment; filename=\"{}\"\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n", // ADDED Content-Type header
            filename, file_size
        );
    }

    if let Err(e) = stream.write_all(response.as_bytes()) {
        if e.kind() == ErrorKind::BrokenPipe {
            println!("[{:?}] Client disconnected (BrokenPipe) during header write for {:?} client {}: Broken pipe", thread::current().id(), path, peer_addr);
            return Ok(()); // Gracefully handle broken pipe
        }
        eprintln!("[{:?}] Error writing header to client {} for {:?}: {}", thread::current().id(), peer_addr, path, e);
        return Err(e.into()); // Propagate other errors
    }


    if start_byte >= file_size {
        send_response(stream, 416, "Requested Range Not Satisfiable", "Range not satisfiable").map_err(|e| format!("Failed to send response to {}: {}", peer_addr, e))?;
        return Ok(());
    }

    let seek_result = file.seek(SeekFrom::Start(start_byte));
    if let Err(e) = seek_result {
        eprintln!("[{:?}] Failed to seek file {:?} for client {} to byte {}: {}", thread::current().id(), path, peer_addr, start_byte, e);
        send_response(stream, 500, "Internal Server Error", "Failed to seek file").map_err(|e| format!("Failed to send error response to {}: {}", peer_addr, e))?;
        return Ok(());
    }

    let mut bytes_remaining = (end_byte - start_byte + 1) as usize;
    let mut read_buffer = vec![0; chunk_size];

    while bytes_remaining > 0 {
        let bytes_to_read = std::cmp::min(bytes_remaining, chunk_size);
        let bytes_read_res = file.read(&mut read_buffer[..bytes_to_read]);
        match bytes_read_res { // Modified to use match and log errors
            Ok(bytes_read) => {
                if bytes_read == 0 {
                    println!("[{:?}] End of file {:?} reached for client {}", thread::current().id(), path, peer_addr);
                    break; // End of file reached
                }
                let write_result = stream.write_all(&read_buffer[..bytes_read]);
                match write_result {
                    Ok(_) => {}, // Successfully wrote chunk
                    Err(e) => {
                        if e.kind() == ErrorKind::BrokenPipe {
                            println!("[{:?}] Client disconnected (BrokenPipe) during file transfer for {:?} client {}", thread::current().id(), path, peer_addr);
                            return Ok(()); // Gracefully handle broken pipe
                        } else {
                            eprintln!("[{:?}] Error writing data chunk to client {} for {:?}: {}", thread::current().id(), peer_addr, path, e);
                            // Explicitly close the stream on *any* write error (except BrokenPipe)
                            println!("[{:?}] Attempting to shutdown and close stream due to write error for client {}", thread::current().id(), peer_addr); // Log before shutdown
                            if let Err(shutdown_err) = stream.shutdown(Shutdown::Both) {
                                eprintln!("[{:?}] Error shutting down stream for client {}: {}", thread::current().id(), peer_addr, shutdown_err);
                            }
                            if let Err(close_err) = stream.take_error() { // Consume and log error from `take_error`
                                eprintln!("[{:?}] Error closing stream (take_error) for client {}: {}", thread::current().id(), peer_addr, close_err);
                            }
                            println!("[{:?}] Stream shutdown and attempted close for client {}", thread::current().id(), peer_addr); // Log after shutdown
                            return Ok(()); // Exit handler, connection is now explicitly closed (or attempted to be)
                        }
                    }
                }
                bytes_remaining -= bytes_read;
            }
            Err(e) => { // Log read errors
                eprintln!("[{:?}] Error reading file {:?} for client {}: {}", thread::current().id(), path, peer_addr, e);
                return Ok(()); // Exit handler for this client on read error
            }
        }
    }
    println!("[{:?}] serve_file finished for: {:?} client: {:?}", thread::current().id(), path, peer_addr);
    Ok(()) // Indicate file serving done successfully
}


fn serve_directory(stream: &mut TcpStream, path: &PathBuf, peer_addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{:?}] serve_directory started for: {:?} client: {:?}", thread::current().id(), path, peer_addr);
    let html_result = generate_directory_listing(path);
    match html_result {
        Ok(html) => {
            send_response(stream, 200, "OK", &html).map_err(|e| format!("Failed to send response to {}: {}", peer_addr, e))?;
        }
        Err(e) => {
            eprintln!("[{:?}] Error generating directory listing for {:?} client {}: {}", thread::current().id(), path, peer_addr, e);
            send_response(stream, 500, "Internal Server Error", "Failed to generate directory listing").map_err(|e| format!("Failed to send error response to {}: {}", peer_addr, e))?;
        }
    }
    println!("[{:?}] serve_directory finished for: {:?} client: {:?}", thread::current().id(), path, peer_addr);
    Ok(()) // Indicate directory serving done successfully
}


fn generate_directory_listing(path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    let read_dir_result = fs::read_dir(path);
    let entries_iter = match read_dir_result {
        Ok(rd) => rd,
        Err(e) => return Err(From::from(format!("Failed to read directory '{}': {}", path.display(), e))),
    };

    let mut entries: Vec<PathBuf> = Vec::new();
    for entry_result in entries_iter {
        match entry_result {
            Ok(entry) => {
                entries.push(entry.path());
            }
            Err(e) => {
                eprintln!("Warning: Skipping entry due to error: {}", e); // Log, but don't fail directory listing entirely
            }
        }
    }
    entries.sort();

    let mut breadcrumbs = String::new();
    let mut current_link = String::from("/");
    for ancestor in path.ancestors().skip(1) {
        if let Some(name_os) = ancestor.file_name() {
            let name = name_os.to_string_lossy();
            breadcrumbs += &format!(
                r#"<li class="breadcrumb-item"><a href="{link}">{name}</a></li>"#,
                link = current_link,
                name = name
            );
            current_link = format!("{}/{}", current_link, name);
        }
    }
    breadcrumbs = breadcrumbs.trim_end_matches('/').to_string();

    let mut table_rows_html = String::new();
    for path in entries {
        let row_html_result = generate_directory_row_html(&path);
        match row_html_result {
            Ok(row_html) => {
                table_rows_html += &row_html;
            }
            Err(e) => {
                eprintln!("Warning: Could not generate table row for path '{}': {}", path.display(), e); // Log, but continue directory listing
            }
        }
    }


    let html = format!(
       r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>Directory Listing for {}</title>
            <link
                href="https://stackpath.bootstrapcdn.com/bootstrap/5.3.0/css/bootstrap.min.css"
                rel="stylesheet"
            >
            <style>
                body {{
                    font-family: 'Inter', sans-serif;
                    background-color: #1a1a1a; /* Material Black background */
                    color: #FFFFFF; /* White text */
                    margin: 0;
                    padding: 20px;
                }}
                .container {{
                    max-width: 960px;
                    margin: 0 auto;
                    padding: 30px;
                    background-color: #424242; /* Darker shade of Material Black */
                    border-radius: 10px;
                    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.7); /* White box shadow with fade effect */
                    transition: box-shadow 0.3s ease-in-out; /* Smooth transition for box shadow */
                }}
                .container:hover {{
                  box-shadow:
                    0px 8px 20px rgba(150, 150, 150, 0.2), /* Bottom shadow */
                    0px -8px 20px rgba(150, 150, 150, 0.2), /* Top shadow */
                    8px 0px 20px rgba(150, 150, 150, 0.2), /* Right shadow */
                    -8px 0px 20px rgba(150, 150, 150, 0.2); /* Left shadow */
                }}
                .breadcrumbs {{
                    list-style: none;
                    padding: 0;
                    margin-bottom: 20px;
                    color: #888888; /* Lighter shade of grey for breadcrumbs */
                }}
                .breadcrumbs li {{
                    display: inline;
                }}
                .breadcrumbs li:after {{
                    content: " / ";
                }}
                .breadcrumbs li:last-child:after {{
                    content: "";
                }}
                h1 {{
                    color: #FF9800; /* Material Orange for heading */
                    margin-bottom: 30px;
                }}
                table {{
                    width: 100%;
                    border-collapse: collapse;
                }}
                th, td {{
                    padding: 10px;
                    text-align: left;
                    border-bottom: 1px solid #555555; /* Slightly lighter border */
                }}
                th {{
                    background-color: #616161; /* Dark grey for header */
                }}
                tr:hover {{
                    background-color: #757575; /* Lighter grey on row hover */
                }}
                a {{
                     color: white; /* Material Yellow for links */
                     text-decoration: none;
                }}
                a:hover {{
                    color: #838fe9;
                    transition: 0.2s;
                    text-decoration: none;
                }}
            </style>
        </head>
        <body>
            <div class="container">
                <ul class="breadcrumbs">
                    {}
                </ul>
                <h1 title={}>Directory Listing</h1>
                <table class="table table-hover">
                    <thead>
                        <tr>
                            <th>Name</th>
                            <th>Size</th>
                            <th>Last Modified</th>
                        </tr>
                    </thead>
                    <tbody>
                        {}
                    </tbody>
                </table>
            </div>
        </body>
        </html>
        "#,
        path.display(),
        breadcrumbs,
        path.display(),
        table_rows_html
    );
    Ok(html)
}

fn generate_directory_row_html(path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    let metadata = fs::metadata(path).map_err(|e| format!("Failed to get metadata for '{}': {}", path.display(), e))?;
    let file_size_human = metadata.len().file_size(options::BINARY).map_err(|e| format!("Failed to format file size for '{}': {}", path.display(), e))?;
    let last_modified: SystemTime = metadata.modified().map_err(|e| format!("Failed to get modification time for '{}': {}", path.display(), e))?;

    let datetime: DateTime<Local> = DateTime::from(last_modified);
    let last_modified_str = datetime.format("%d-%m-%Y %H:%M:%S").to_string();

    let current_dir = path.parent().ok_or_else(|| format!("Path '{}' has no parent", path.display()))?;
    let relative_path = path.strip_prefix(current_dir).map_err(|e| format!("Failed to strip prefix from path '{}': {}", path.display(), e))?;
    let filename_os = path.file_name().ok_or_else(|| format!("No filename for path {:?}", path))?;
    let filename = filename_os.to_string_lossy();


    Ok(format!(
        "<tr><td><a href=\"{}\">{}</a></td><td>{}</td><td>{}</td></tr>",
        percent_encode_path(relative_path),
        filename,
        file_size_human,
        last_modified_str
    ))
}


// Helper function to percent-encode path segments for URLs - important for directory listing links
fn percent_encode_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None, // Skip RootDir, ParentDir, CurDir, Prefix components
        })
        .collect::<Vec<_>>()
        .join("/")
        .replace(" ", "%20") // Basic space encoding, consider more robust URL encoding if needed
}


fn send_response(stream: &mut TcpStream, status_code: u16, status_text: &str, body: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("[{:?}] send_response - Status: {}, Body Length: {}", thread::current().id(), status_code, body.len());
    let image_map = [
        (400, "error_400.dat"),
        (403, "error_403.dat"),
        (404, "error_404.dat"),
    ];

    let (content_type, response_body) =
        if let Some((_, image_name)) = image_map.iter().find(|(code, _)| *code == status_code) {
            match Assets::get(image_name) {
                Some(embedded_file) => ("image/png", embedded_file.data.into_owned()),
                None => (
                    "text/plain",
                    format!("Error {}: {}. Image not found.", status_code, status_text)
                        .as_bytes()
                        .to_vec(),
                ),
            }
        } else {
            ("text/html; charset=utf-8", body.as_bytes().to_vec())
        };

    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status_code,
        status_text,
        content_type,
        response_body.len()
    );

    stream.write_all(response.as_bytes()).map_err(|e| format!("Failed to write response header: {}", e))?;
    stream.write_all(&response_body).map_err(|e| format!("Failed to write response body: {}", e))?;
    Ok(())
}