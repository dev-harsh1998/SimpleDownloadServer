/*
 * SPDX-License-Identifier: GPL-3.0-or-later
 * More licensing information can be found in the project LICENSE file
 * Author: Harshit Jain
 * Email: reach@harsh1998.dev
 */

use chrono::{DateTime, Local, TimeZone};
use clap::Parser;
use humansize::{file_size_opts as options, FileSize};
use rust_embed::RustEmbed;
use std::fs::{self, File};
use std::io::{prelude::*, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::UNIX_EPOCH;

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
    /// Allowed file extensions for download (comma-separated)
    #[arg(short, long, default_value = "zip,txt")]
    allowed_extensions: String,
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
            .map(|ext| ext.trim().to_string())
            .collect(),
    );

    let listener = TcpListener::bind(format!("{}:{}", cli.listen, cli.port)).unwrap();
    println!(
        "Listening on {}:{} for directory {} (allowed extensions: {:?})",
        cli.listen,
        cli.port,
        file_directory.lock().unwrap().to_string(),
        allowed_extensions
    );

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let file_directory = Arc::clone(&file_directory);
                let allowed_extensions = Arc::clone(&allowed_extensions);
                thread::spawn(move || {
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
    download_extensions: &Arc<Vec<String>>,
) {
    let buf_reader = BufReader::new(&mut stream);

    let request_line = match buf_reader.lines().next() {
        Some(Ok(line)) => line,
        Some(Err(e)) => {
            eprintln!("Error reading request line: {}", e);
            send_response(
                &mut stream,
                400,
                "Bad Request",
                "Error reading request line",
            );
            return;
        }
        None => {
            send_response(&mut stream, 400, "Bad Request", "Empty request");
            return;
        }
    };

    let requested_path = request_line.split_whitespace().nth(1);

    let file_directory = file_directory.lock().unwrap();

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

    let file_extension_allowed = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(|ext| download_extensions.iter().any(|allowed| allowed == ext))
        .unwrap_or(false);

    if !path.is_dir() && file_extension_allowed {
        if let Ok(mut file) = File::open(&path) {
            let file_size = file.metadata().unwrap().len();
            let filename = path.file_name().unwrap_or_default().to_string_lossy();
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Disposition: attachment; filename=\"{filename}\"\r\nContent-Length: {file_size}\r\n\r\n").as_bytes()).unwrap();

            const BUFFER_SIZE: usize = 1024 * 1024;
            let mut buffer = [0; BUFFER_SIZE];
            loop {
                let bytes_read = file.read(&mut buffer).unwrap();
                if bytes_read == 0 {
                    break;
                }
                stream.write_all(&buffer[..bytes_read]).unwrap();
            }
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
                r#"<li><a href="{link}">{name}</a></li>"#,
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
            <style>
                body {{
                    font-family: 'Inter', sans-serif; 
                    /* Gradient background - adjust colors to your liking */
                    background: linear-gradient(135deg, #f2e7fe, #e0c3fc);
                    color: #333;          
                    margin: 0;
                    padding: 20px;
                }}
                .container {{
                    max-width: 960px;
                    margin: 0 auto;        
                    padding: 30px;
                    background-color: #fff; /* White container */
                    border-radius: 10px;     
                    box-shadow: 0 4px 8px rgba(0, 0, 0, 0.1); 
                }}
                h1 {{
                    color: #6829e0;       /* Purple header */
                    text-align: center;
                    margin-bottom: 30px;
                }}
                .breadcrumbs {{
                    list-style: none;
                    padding: 0;
                    margin-bottom: 20px;
                }}
                .breadcrumbs li {{
                    display: inline;
                }}
                .breadcrumbs li:after {{
                    content: " / ";
                    color: #888;        
                }}
                .breadcrumbs li:last-child:after {{
                    content: "";
                }}
                table {{
                    width: 100%;
                    border-collapse: collapse;
                    table-layout: fixed; 
                }}
                th, td {{
                    padding: 15px;
                    text-align: left;
                    border-bottom: 1px solid #eee; 
                }}
                th {{
                    background-color: #ddd; /* Light gray header */
                    color: #333;
                    font-weight: 600;   
                }}
                tr:hover {{
                    background-color: #f5f5f5; 
                }}
                a {{
                    text-decoration: none;
                    color: #6829e0; /* Purple links */
                    transition: color 0.2s; 
                }}
                a:hover {{
                    color: #ff9800; /* Orange on hover */
                }}

                /* Table column widths (adjust as needed) */
                .name-col {{ width: 60%; }}
                .size-col {{ width: 20%; }}
                .date-col {{ width: 20%; }} 

            </style>
        </head>
        <body>
            <div class="container">
                <ul class="breadcrumbs">{}</ul>
                <h1>Directory Listing for {}</h1>
                <table>
                    <tr><th class="name-col">Name</th><th class="size-col">Size</th><th class="date-col">Last Modified</th></tr>
                    {}
                </table>
            </div>
        </body>
        </html>
        "#,
        path.display(),
        breadcrumbs,
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

    stream.write_all(response.as_bytes()).unwrap();
    stream.write_all(&response_body).unwrap();
}