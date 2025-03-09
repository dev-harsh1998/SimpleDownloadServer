/*
 * SPDX-License-Identifier: GPL-3.0-or-later
 * More licensing information can be found in the project LICENSE file
 * Author: Harshit Jain
 * Email: reach@harsh1998.dev
 */
use chrono::{DateTime, Local, TimeZone};
use clap::Parser;
use glob::Pattern;
use humansize::{file_size_opts as options, FileSize};
use rust_embed::RustEmbed;
use std::fs::{self, File};
use std::io::{prelude::*, BufReader, Read, Seek, SeekFrom};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::str::FromStr;
use std::time::UNIX_EPOCH;
use threadpool::ThreadPool;

#[derive(RustEmbed)]
#[folder = "assets"]
struct Assets;

#[derive(Parser)]
#[command(
    author = "Harshit Jain",
    version = "1.0.0",
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
    #[arg(short, long, default_value_t = 4)]
    threads: usize,
}

fn main() {
    let cli = Cli::parse();
    let file_directory = Arc::new(Mutex::new(
        PathBuf::from(cli.directory)
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string(),
    ));
    let allowed_extensions = Arc::new(
        cli.allowed_extensions
            .split(',')
            .map(|ext| Pattern::new(ext.trim()).unwrap())
            .collect::<Vec<Pattern>>(),
    );

    let listener = TcpListener::bind(format!("{}:{}", cli.listen, cli.port)).unwrap();
    println!(
        "Listening on {}:{} for directory {} (allowed extensions: {:?})",
        cli.listen,
        cli.port,
        file_directory.lock().unwrap_or_else(|e| e.into_inner()).to_string(),
        allowed_extensions
    );

    let pool = ThreadPool::new(cli.threads);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let file_directory = Arc::clone(&file_directory);
                let allowed_extensions = Arc::clone(&allowed_extensions);
                pool.execute(move || {
                    handle_client(stream, &file_directory, &allowed_extensions);
                });
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
            }
        }
    }
}

fn handle_client(
    mut stream: TcpStream,
    file_directory: &Arc<Mutex<String>>,
    download_extensions: &Arc<Vec<Pattern>>,
) {
    let mut buffer = [0; 1024];
    let mut request_line = String::new();
    let mut headers = Vec::new();

    // Read the request line
    let bytes_read = stream.read(&mut buffer).unwrap();
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let mut lines = request.lines();

    if let Some(line) = lines.next() {
        request_line = line.to_string();
    } else {
        send_response(&mut stream, 400, "Bad Request", "Empty request");
        return;
    }

    // Read the headers
    for line in lines {
        if line.is_empty() {
            break;
        }
        println!("{}", line);
        headers.push(line.to_string());
    }

    let requested_path = request_line.split_whitespace().nth(1);

    let file_directory = file_directory.lock().unwrap_or_else(|e| e.into_inner());

    let file_directory_path = PathBuf::from(&*file_directory);

    let path = match requested_path {
        Some(path) if path.starts_with('/') => {
            file_directory_path.join(path.trim_start_matches('/'))
        }
        _ => {
            send_response(&mut stream, 400, "Bad Request", "Invalid request path");
            return;
        }
    };

    if !path.exists() {
        send_response(&mut stream, 404, "Not Found", "File or directory not found");
        return;
    }

    if !path.starts_with(&*file_directory) {
        send_response(&mut stream, 403, "Forbidden", "Access denied");
        return;
    }

    let file_extension_allowed = if let Some(ext) = path.extension().and_then(std::ffi::OsStr::to_str) {
        download_extensions.iter().any(|pattern| pattern.matches(&format!(".{}", ext)))
    } else {
        // Allow files with no extension if no patterns are specified or if a pattern allows it
        download_extensions.is_empty() || download_extensions.iter().any(|pattern| pattern.matches(""))
    };

    if !path.is_dir() && file_extension_allowed {
        if let Ok(mut file) = File::open(&path) {
            let file_size = file.metadata().unwrap().len();
            let filename = path.file_name().unwrap_or_default().to_string_lossy();

            let mut range_header = String::new();
            for header in headers {
                if header.starts_with("Range: bytes=") {
                    range_header = header.trim_start_matches("Range: bytes=").to_string();
                    break;
                }
            }

            let mut start_byte = 0;
            let mut end_byte = file_size - 1;

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
            }

            let content_length = end_byte - start_byte + 1;

            if start_byte >= file_size {
                send_response(&mut stream, 416, "Requested Range Not Satisfiable", "Range not satisfiable");
                return;
            }

            file.seek(SeekFrom::Start(start_byte)).unwrap();

            let mut buffer = vec![0; content_length as usize];
            file.read_exact(&mut buffer).unwrap();

            let response = format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {}-{}/{}\r\nContent-Disposition: attachment; filename=\"{filename}\"\r\nContent-Length: {content_length}\r\n\r\n",
                start_byte, end_byte, file_size
            );

            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(&buffer).unwrap();
        } else {
            send_response(&mut stream, 404, "Not Found", "File not found");
        }
    } else if path.is_dir() {
        let html = generate_directory_listing(&path);
        send_response(&mut stream, 200, "OK", &html);
    } else {
        send_response(
            &mut stream,
            403,
            "Forbidden",
            "Only allowed files can be downloaded",
        );
    }
}

fn generate_directory_listing(path: &PathBuf) -> String {
    let mut entries: Vec<_> = fs::read_dir(path)
        .unwrap_or_else(|_| panic!("Unable to read directory: {:?}", path))
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    entries.sort();

    let mut breadcrumbs = String::new();
    let mut current_link = String::from("/");
    for ancestor in path.ancestors().skip(1) {
        if let Some(name) = ancestor.file_name() {
            breadcrumbs += &format!(
                r#"<li class="breadcrumb-item"><a href="{link}">{name}</a></li>"#,
                link = current_link,
                name = name.to_string_lossy()
            );
            current_link = format!("{}/{}", current_link, name.to_string_lossy());
        }
    }
    breadcrumbs = breadcrumbs.trim_end_matches('/').to_string();

    let html = format!(
       r#"
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="UTF-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <title>Directory Listing for {}</title>
            <!-- Bootstrap CSS -->
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
        path.display(),
        entries
            .iter()
            .map(|path| {
                let metadata = fs::metadata(path).unwrap();
                let file_size = metadata.len().file_size(options::BINARY).unwrap(); // Format file size
                let last_modified = metadata
                    .modified()
                    .unwrap()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let naive_datetime =
                    chrono::NaiveDateTime::from_timestamp_opt(last_modified as i64, 0).unwrap();
                let datetime: DateTime<Local> = Local.from_local_datetime(&naive_datetime).unwrap();
                let last_modified_str = datetime.format("%d-%m-%Y %H:%M:%S").to_string(); // format the date and time

                let current_dir = path.parent().unwrap();

                let relative_path = path.strip_prefix(current_dir).unwrap();

                format!(
                    "<tr><td><a href=\"{}\">{}</a></td><td>{}</td><td>{}</td></tr>",
                    relative_path.display(),
                    path.file_name().unwrap().to_string_lossy(),
                    file_size,
                    last_modified_str
                )
            })
            .collect::<String>()
    );
    html
}

fn send_response(stream: &mut TcpStream, status_code: u16, status_text: &str, body: &str) {
    let image_map = [
        (400, "error_400.dat"),
        (403, "error_403.dat"),
        (404, "error_404.dat"),
    ];

    let (content_type, response_body) =
        if let Some(image_name) = image_map.iter().find(|(code, _)| *code == status_code) {
            match Assets::get(image_name.1) {
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
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
        status_code,
        status_text,
        content_type,
        response_body.len()
    );

    stream.write_all(response.as_bytes()).unwrap_or_else(|e| {
        eprintln!("Error writing response: {}", e);
    });
    stream.write_all(&response_body).unwrap_or_else(|e| {
        eprintln!("Error writing response body: {}", e);
    });
}
