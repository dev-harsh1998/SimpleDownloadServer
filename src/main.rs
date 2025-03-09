/*
 *  Simple Download Server - Human-Readable Comments Edition! 🚀
 *
 *  This server is designed to be a friendly file-sharing tool. 🤝
 *  Think of it as your personal cloud, but simpler and more configurable. ⚙️
 *  It serves files from a specific directory you choose, making it easy to share with friends or colleagues.
 *
 *  More licensing information can be found in the project LICENSE file - it's important stuff! 📜
 *  Author: Harshit Jain - the person who built this cool tool. 🧑‍💻
 *  Email: reach@harsh1998.dev - if you have questions or want to say hi! 👋
 */

use chrono::{DateTime, Local}; // For handling dates and times - like "last modified". ⏰
use clap::Parser; // Makes command-line arguments easy to handle. 📝
use glob::Pattern; // For those fancy file extension patterns like "*.zip". ✨
use humansize::{file_size_opts as options, FileSize}; // Makes file sizes like "1.2 MB" instead of just bytes. 📊
use log::{debug, error, info, warn}; // For logging messages - like a server diary. 📒
use rust_embed::RustEmbed; // Embeds assets (like error images) directly into the compiled program. 📦
use std::collections::HashMap; // For storing headers - like a key-value store. 🔑 📦
use std::fs::{self, File}; // For interacting with the file system - reading files and directories. 📁
use std::io::{prelude::*, BufReader, ErrorKind, Read, Seek, SeekFrom}; // For input/output operations - reading requests and sending responses. 📤📥
use std::net::{Shutdown, TcpListener, TcpStream}; // For networking stuff - listening for connections and handling them. 🌐
use std::path::{Component, Path, PathBuf}; // For working with file paths. 🗂️
use std::str::FromStr; // For converting strings to other types. ➡️
use std::sync::{Arc, Mutex}; // For sharing data safely between threads. 🧵
use std::thread; // For running code in separate threads - like handling multiple requests. 🧵
use std::time::SystemTime; // For getting system time - used for "last modified". ⏱️
use threadpool::ThreadPool; // For making the server handle multiple requests at once efficiently. 🏊‍♀️🏊🏊‍♂️

// Embeds files from the "assets" directory into the binary.
// This is useful for including things like error images directly in the server executable.
#[derive(RustEmbed)]
#[folder = "assets"]
struct Assets;

// Defines the command-line interface using clap. 🎉
// This struct represents the structure of arguments you can pass when running the server.
#[derive(Parser)]
#[command(
     author = "Harshit Jain",
     version = "1.7.0", //  Version of our Simple Download Server - feels like we're shipping software! 🚢
     long_about = "This is a simple configurable download server that serves files from a directory with sophisticated error reporting and handling.\n\
 It can be used to share files with others or to download files from a remote server.\n\
 The server can be configured to serve only specific file extensions and can be run on a specific host and port.\n\
 If the requested path is a directory, the server will generate an HTML page with a list of files and subdirectories in the directory.\n\
 The server will respond with detailed error logs for various scenarios, enhancing operational visibility.\n\
 The server can be configured to serve only specific file extensions and can be run on a specific host and port.\n\
 The server will respond with a 403 Forbidden error if the requested file extension is not allowed.\n\
 The server will respond with a 404 Not Found error if the requested file or directory does not exist.\n\
 The server will respond with a 400 Bad Request error if the request is invalid.\n\
 Follow & conribute with devlopment efforts at: git.harsh1998.dev \n\
 Author: Harshit Jain, UI Design by: Sonu Kr. Saw\n",
     about = "A simple configurable download server with sophisticated error reporting." // Short description for `hdl_sv --help`.
 )]
struct Cli {
    /// Directory path to serve, mandatory -  This is the *only* required argument. 📂
    #[arg(short, long, required = true)]
    directory: PathBuf,

    /// Host address to listen on (e.g., "127.0.0.1" for local, "0.0.0.0" for everyone on the network). 👂
    #[arg(short, long, default_value = "127.0.0.1")]
    listen: String,

    /// Port number to listen on -  Like a door number for the server to receive requests. 🚪
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    /// Allowed file extensions for download (comma-separated, supports wildcards like *.zip, *.txt) -  Security measure to only share certain file types. 🔒
    #[arg(short, long, default_value = "*.zip,*.txt")]
    allowed_extensions: String,

    /// Number of threads in the thread pool -  More threads = handle more downloads at once, up to a point. 🧵🧵🧵
    #[arg(short, long, default_value_t = 8)]
    threads: usize,

    /// Chunk size for reading files (in bytes) -  How much data we read from a file at a time when sending it. Smaller chunks are gentler on memory. 📦
    /// This is the size of the buffer used to read files in chunks
    #[arg(short, long, default_value_t = 1024)]
    chunk_size: usize,

    /// Enable verbose logging for debugging (log level: debug) -  For super detailed logs, useful when things go wrong or you're developing. 🐛
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    /// Enable more detailed logging (log level: info if verbose=false, debug if verbose=true) -  More logs than usual, but not *too* much. Good for general monitoring. ℹ️
    #[arg(long, default_value_t = false)]
    detailed_logging: bool,
}

fn main() {
    // Parse command-line arguments. 🚀
    let cli = Cli::parse();

    // Initialize logging based on command-line flags. 📝
    let log_level = if cli.verbose {
        "debug" // Super detailed logs for debugging. 🔍
    } else if cli.detailed_logging {
        "info" //  Informative logs - good for monitoring. ℹ️
    } else {
        "warn" //  Only warnings and errors - default, less noisy. ⚠️
    };

    // Configure the logging environment. 🌳
    if std::env::var("RUST_LOG").is_err() {
        // Only set RUST_LOG if it's not already defined by the user.
        // This prevents overriding user's custom logging settings.
        std::env::set_var("RUST_LOG", log_level);
    }
    env_logger::init(); // Initialize the logger with the configured settings.

    debug!("Log level set to: {}", log_level); // Log the determined log level at debug level - just for our info. ⚙️

    // Run the server and handle any errors gracefully. 🛡️
    if let Err(e) = run_server(cli) {
        error!("Server error: {}", e); // Log server errors using the error! macro - something went seriously wrong. 🚨
        std::process::exit(1); // Exit the program with an error code. 💀
    }
}

// Main function to run the server. 🚀
fn run_server(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Prepare the directory to be served. 📂
    // Arc and Mutex are for safe sharing across threads. 🧵
    let file_directory = Arc::new(Mutex::new(
        PathBuf::from(cli.directory.clone())
            .canonicalize()
            .map_err(|e| {
                // If we can't canonicalize (make the path absolute and resolve symlinks), log an error and return. 🙁
                error!(
                    "Failed to canonicalize directory path '{}': {}",
                    cli.directory.display(),
                    e
                );
                format!("Failed to canonicalize directory path: {}", e)
            })?, // ? operator propagates the error up if canonicalization fails.
    ));

    // Validate that the directory exists and is accessible. ✅
    if !is_directory(&file_directory).map_err(|e| {
        // If the directory check fails, log an error and return. 🙁
        error!(
            "Error checking directory '{}': {}",
            cli.directory.display(),
            e
        );
        format!("Error checking directory: {}", e)
    })? {
        // If it's not a directory or not accessible, create an error message. ❌
        let err_msg = format!(
            "Directory '{}' does not exist or is not accessible.",
            cli.directory.display()
        );
        error!("{}", err_msg); // Log the error.
        return Err(From::from(err_msg)); // Return the error to the caller.
    }

    // Parse allowed file extensions into glob patterns. ✨
    let allowed_extensions = Arc::new(
        cli.allowed_extensions
            .split(',') // Split the comma-separated string into individual extensions.
            .map(|ext| {
                Pattern::new(ext.trim()).map_err(|e| {
                    // For each extension, create a glob pattern (for wildcard matching).
                    // If pattern creation fails (e.g., invalid pattern), log and return an error. 🙁
                    error!("Invalid extension pattern '{}': {}", ext.trim(), e);
                    format!("Invalid extension pattern '{}': {}", ext.trim(), e)
                })
            })
            .collect::<Result<Vec<Pattern>, String>>() // Collect all patterns into a vector, or return an error if any pattern failed.
            .map_err(|e| {
                // If collecting patterns failed at any point, log and return an error. 🙁
                error!(
                    "Error parsing allowed extensions '{}': {}",
                    cli.allowed_extensions, e
                );
                format!("Error parsing allowed extensions: {}", e)
            })?, // ? operator propagates the error up if pattern creation or collection fails.
    );

    // Construct the address to bind to (host:port). 🌐
    let bind_address = format!("{}:{}", cli.listen, cli.port);

    // Create a TCP listener to accept incoming connections. 👂
    let listener = TcpListener::bind(&bind_address).map_err(|e| {
        // If binding fails (e.g., port already in use), log an error and return. 🙁
        error!("Failed to bind to address '{}': {}", bind_address, e);
        format!(
            "Failed to bind to address {}:{}: {}",
            cli.listen, cli.port, e
        )
    })?; // ? operator propagates the error up if binding fails.

    info!(
        "Server listening on {} for directory '{}' (allowed extensions: {:?})",
        bind_address,
        file_directory.lock().unwrap().display(), // Display the directory we are serving (after getting the lock).
        allowed_extensions                        // Display the list of allowed extensions.
    );

    // Initialize a thread pool to handle incoming connections concurrently. 🧵🧵🧵
    let pool = ThreadPool::new(cli.threads);
    debug!("Thread pool initialized with {} threads.", cli.threads); // Log the thread pool size at debug level. ⚙️

    // Start listening for incoming TCP connections in a loop. 🔄
    for stream_result in listener.incoming() {
        match stream_result {
            Ok(stream) => {
                // If we successfully get a TCP stream (connection). 🎉
                let file_directory_arc = Arc::clone(&file_directory); // Clone the directory Arc to move into the thread.
                let allowed_extensions_arc = Arc::clone(&allowed_extensions); // Clone the allowed extensions Arc to move into the thread.
                let chunk_size = cli.chunk_size; // Copy chunk size - it's Copy, so no Arc needed.

                // Get the peer address (client's address). 🌐
                let peer_addr_result = stream.peer_addr().map_err(|e| {
                    warn!("Failed to get peer address for incoming connection: {}", e); // Log a warning if we can't get the peer address. ⚠️
                    "unknown_peer".to_string() // Fallback to "unknown_peer" string if we can't resolve it.
                });
                let peer_addr_string = match peer_addr_result {
                    Ok(addr) => addr.to_string(), // Convert SocketAddr to String for easier use in logging and handle_client.
                    Err(fallback_str) => fallback_str, // Use the fallback string if getting peer address failed.
                };

                // Clone peer_addr_string for use in the move closure. 🔑
                // This prevents a move error because the closure takes ownership.
                let peer_addr_string_clone = peer_addr_string.clone();

                // Execute the handle_client function in a separate thread from the thread pool. 🧵
                pool.execute(move || {
                    // This code will run in a thread from the pool. 🚀
                    debug!(
                        "[Peer: {}] Handling client connection in thread {:?}",
                        peer_addr_string_clone, // Use the clone for logging.
                        thread::current().id()
                    ); // Log that we are handling a client connection in a thread. ⚙️

                    // Handle the client connection. 🤝
                    if let Err(e) = handle_client(
                        stream,
                        &file_directory_arc,
                        &allowed_extensions_arc,
                        chunk_size,
                        peer_addr_string_clone.clone(), // Clone again for handle_client - it takes ownership.
                    ) {
                        // If handle_client returns an error, log it as an error. 🚨
                        error!(
                            "[Peer: {}] Error handling client: {}",
                            peer_addr_string_clone, // Use the clone for error logging.
                            e
                        );
                    } else {
                        debug!(
                            "[Peer: {}] Client handled successfully.",
                            peer_addr_string_clone // Use the clone for success logging.
                        ); // Log successful client handling. ⚙️
                    }
                });
            }
            Err(e) => {
                // If accepting a connection fails, log an error. 🚨
                error!("Error accepting connection: {}", e);
            }
        }
    }
    info!("Server shutting down gracefully."); // Log server shutdown at info level - server is stopping. ℹ️
    Ok(()) // Return Ok to signal successful server execution. ✅
}

// Checks if the given path (locked PathBuf) is a directory. 📂
fn is_directory(file_directory: &Arc<Mutex<PathBuf>>) -> Result<bool, String> {
    // Lock the mutex to access the PathBuf safely. 🔒
    let dir_guard_result = file_directory.lock().map_err(|e| {
        // If locking the mutex fails, log an error and return. 🙁
        error!(
            "Failed to lock directory mutex while checking directory: {}",
            e
        );
        "Failed to lock directory mutex".to_string()
    })?; // ? operator propagates the error if locking fails.

    // Check if the path (dereferenced from the mutex guard) is a directory and return the result. ✅
    Ok(Path::new(&*dir_guard_result).is_dir())
}

// Handles a single client connection. 🤝
fn handle_client(
    mut stream: TcpStream, // Mutable TCP stream for sending responses. 📤
    file_directory: &Arc<Mutex<PathBuf>>, // Shared directory path. 📂
    download_extensions: &Arc<Vec<Pattern>>, // Shared list of allowed file extensions. ✨
    chunk_size: usize,     // Chunk size for file reading. 📦
    peer_addr: String,     // Peer address as a string for logging. 🌐
) -> Result<(), Box<dyn std::error::Error>> {
    debug!(
        "[Peer: {}][ThreadId({:?})] handle_client started.",
        peer_addr,
        thread::current().id()
    ); // Log client handling start at debug level. ⚙️

    // Create a buffered reader for efficient reading from the TCP stream. 📖
    let reader = BufReader::new(&stream);
    let mut lines_iter = reader.lines(); // Get an iterator over the lines of the request.

    // Read the request line (e.g., "GET / HTTP/1.1"). 📜
    let request_line_option = lines_iter.next();

    let request_line: String;
    match request_line_option {
        Some(Ok(line)) => {
            // If we successfully read a line, store it as the request line. 🎉
            request_line = line.to_string();
        }
        Some(Err(e)) => {
            // If there's an error reading the request line, log a warning and return the error. ⚠️
            warn!(
                "[Peer: {}][ThreadId({:?})] Error reading request line: {}",
                peer_addr,
                thread::current().id(),
                e
            );
            return Err(e.into()); // Convert the IO error to a boxed error and return.
        }
        None => {
            // If we get None, it means the request was empty. 🙁
            warn!(
                "[Peer: {}][ThreadId({:?})] Empty request received.",
                peer_addr,
                thread::current().id()
            ); // Log a warning about the empty request. ⚠️
            send_response(&mut stream, 400, "Bad Request", "Empty request", &peer_addr).map_err(
                |e| {
                    // Send a 400 Bad Request response. If sending fails, log an error. 🚨
                    error!(
                        "[Peer: {}][ThreadId({:?})] Failed to send 400 Bad Request response: {}",
                        peer_addr,
                        thread::current().id(),
                        e
                    );
                    format!("Failed to send 400 Bad Request response: {}", e)
                },
            )?; // ? operator propagates the error if sending response fails.
            return Ok(()); // Return Ok because we've handled the error by sending a response. ✅
        }
    };

    debug!(
        "[Peer: {}][ThreadId({:?})] Request line: {}",
        peer_addr,
        thread::current().id(),
        request_line
    ); // Log the received request line at debug level. ⚙️

    // Extract the requested path from the request line. 🗺️
    let request_path = get_request_path(&request_line);
    debug!(
        "[Peer: {}][ThreadId({:?})] Parsed request path: '{}'",
        peer_addr,
        thread::current().id(),
        request_path
    ); // Log the parsed request path at debug level. ⚙️

    // Prepare a HashMap to store request headers. 🔑
    let mut headers_map = HashMap::new();
    debug!(
        "[Peer: {}][ThreadId({:?})] Reading headers...",
        peer_addr,
        thread::current().id()
    ); // Log header reading start at debug level. ⚙️

    // Read headers line by line until an empty line is encountered (end of headers). 📖
    loop {
        let header_line_option = lines_iter.next(); // Read the next line (potential header).
        match header_line_option {
            Some(Ok(line)) => {
                // If we read a line successfully. 🎉
                if line.is_empty() || line == "\r" {
                    // An empty line signals the end of headers. 🏁
                    debug!(
                        "[Peer: {}][ThreadId({:?})] End of headers.",
                        peer_addr,
                        thread::current().id()
                    ); // Log header reading completion at debug level. ⚙️
                    break; // Exit the loop as headers are finished.
                }
                debug!(
                    "[Peer: {}][ThreadId({:?})] Header line: {}",
                    peer_addr,
                    thread::current().id(),
                    line
                ); // Log each header line at debug level. ⚙️

                // Parse header line into key and value. 🔑
                if let Some(colon_index) = line.find(':') {
                    // Find the colon separating key and value.
                    let key = line[..colon_index].trim(); // Header key is before the colon, trimmed.
                    let value = line[colon_index + 1..].trim(); // Header value is after the colon, trimmed.
                    headers_map.insert(key.to_string(), value.to_string()); // Store the header in the HashMap.
                } else {
                    // If a header line doesn't have a colon, it's invalid. ⚠️
                    warn!(
                        "[Peer: {}][ThreadId({:?})] Invalid header line (no colon): {}",
                        peer_addr,
                        thread::current().id(),
                        line
                    ); // Log a warning about the invalid header line. ⚠️
                }
            }
            Some(Err(e)) => {
                // If there's an error reading a header line, log a warning and break. ⚠️
                warn!(
                    "[Peer: {}][ThreadId({:?})] Error reading header line: {}",
                    peer_addr,
                    thread::current().id(),
                    e
                );
                break; // Stop reading headers if there's an error.
            }
            None => {
                // If we get None while reading headers, it means connection closed prematurely. ⚠️
                warn!(
                     "[Peer: {}][ThreadId({:?})] Connection closed prematurely while reading headers.",
                     peer_addr,
                     thread::current().id()
                 ); // Log a warning about premature connection closure. ⚠️
                break; // Stop reading headers because connection is closed.
            }
        }
    }

    // Determine what to serve based on the request path. 🗺️
    if request_path == "/" {
        // If the path is just "/", serve the directory listing of the root directory. 📂
        serve_directory(
            &mut stream,
            file_directory.lock().unwrap().clone(), // Clone the directory PathBuf for serve_directory.
            &peer_addr,
        )?; // Call serve_directory function.
    } else {
        // For any other path, try to serve a file. 📄
        let full_path = file_directory.lock().unwrap().join(request_path); // Construct the full path by joining requested path with server directory.

        // Check if the full path is a directory. 📂
        if full_path.is_dir() {
            // If it's a directory, serve a directory listing for it. 📂
            serve_directory(&mut stream, full_path, &peer_addr)?; // Call serve_directory for the subdirectory.
        } else if download_extensions
            .iter()
            .any(|pattern| pattern.matches_path(&PathBuf::from(request_path)))
        {
            // Check if the file extension is allowed based on configured patterns. ✨
            serve_file(&mut stream, &full_path, headers_map, chunk_size, &peer_addr)?;
        // Serve the file using serve_file function.
        } else {
            // If it's not a directory and not an allowed file extension, it's "Not Found" or forbidden. ❌
            warn!(
                 "[Peer: {}][ThreadId({:?})] File extension not allowed or file not found for path: '{}'",
                 peer_addr,
                 thread::current().id(),
                 request_path
             ); // Log a warning about disallowed extension or file not found. ⚠️
            send_response(
                &mut stream,
                404,
                "Not Found",
                "File not found or extension not allowed",
                &peer_addr,
            )
            .map_err(|e| {
                // Send a 404 Not Found response. If sending fails, log an error. 🚨
                error!(
                    "[Peer: {}][ThreadId({:?})] Failed to send 404 Not Found response: {}",
                    peer_addr,
                    thread::current().id(),
                    e
                );
                format!("Failed to send 404 Not Found response: {}", e)
            })?; // ? operator propagates the error if sending response fails.
        }
    }

    debug!(
        "[Peer: {}][ThreadId({:?})] handle_client finished.",
        peer_addr,
        thread::current().id()
    ); // Log client handling completion at debug level. ⚙️
    Ok(()) // Return Ok to indicate successful client handling. ✅
}

// Serves a file to the client, handling range requests, etc. 📄
pub fn serve_file(
    stream: &mut TcpStream,           // Mutable TCP stream to send file data. 📤
    path: &PathBuf,                   // Path to the file to be served. 📄
    headers: HashMap<String, String>, // Request headers - we'll check for "Range". 🔑
    chunk_size: usize,                // Chunk size for reading file data. 📦
    peer_addr: &str,                  // Peer address for logging. 🌐
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "[Peer: {}][ThreadId({:?})] serve_file started for: '{}'",
        peer_addr,
        thread::current().id(),
        path.display()
    ); // Log file serving start at info level. ℹ️
    debug!(
        "[Peer: {}][ThreadId({:?})] Opening file: '{}'",
        peer_addr,
        thread::current().id(),
        path.display()
    ); // Log file opening attempt at debug level. ⚙️

    // Try to open the file. 🔓
    let file_result = File::open(path);
    let mut file = match file_result {
        Ok(f) => f, // If file opens successfully, assign it to 'file'. 🎉
        Err(e) => {
            // If file opening fails, handle the error. 🙁
            let err_msg = format!("Failed to open file '{}': {}", path.display(), e);
            error!(
                "[Peer: {}][ThreadId({:?})] {}",
                peer_addr,
                thread::current().id(),
                err_msg
            ); // Log file opening error. 🚨
            let status_code = if e.kind() == ErrorKind::NotFound {
                404 // 404 Not Found if the file doesn't exist.
            } else {
                500 // 500 Internal Server Error for other file opening errors.
            };
            let status_text = if status_code == 404 {
                "Not Found"
            } else {
                "Internal Server Error"
            };
            let body = if status_code == 404 {
                "File not found"
            } else {
                "Failed to open file"
            };
            send_response(stream, status_code, status_text, body, peer_addr).map_err(
                |send_err| {
                    // Send an error response to the client. If sending fails, log error. 🚨
                    error!(
                        "[Peer: {}][ThreadId({:?})] Failed to send error response ({}-{}): {}",
                        peer_addr,
                        thread::current().id(),
                        status_code,
                        status_text,
                        send_err
                    );
                    format!("Failed to send error response: {}", send_err)
                },
            )?; // ? operator propagates error if sending fails.
            return Ok(()); // Return Ok because we've handled the error by sending a response. ✅
        }
    };

    debug!(
        "[Peer: {}][ThreadId({:?})] File opened successfully: '{}'",
        peer_addr,
        thread::current().id(),
        path.display()
    ); // Log successful file opening at debug level. ⚙️

    // Get file metadata (size, modification time, etc.). 🗂️
    let metadata_result = file.metadata();
    let file_size: u64 = match metadata_result {
        Ok(metadata) => metadata.len(), // Get file size from metadata if successful. 🎉
        Err(e) => {
            // If getting metadata fails, handle the error. 🙁
            let err_msg = format!("Failed to get metadata for '{}': {}", path.display(), e);
            error!(
                "[Peer: {}][ThreadId({:?})] {}",
                peer_addr,
                thread::current().id(),
                err_msg
            ); // Log metadata error. 🚨
            send_response(
                 stream,
                 500,
                 "Internal Server Error",
                 "Failed to read file metadata",
                 peer_addr,
             )
             .map_err(|send_err| {
                 // Send 500 error response. If sending fails, log error. 🚨
                 error!(
                     "[Peer: {}][ThreadId({:?})] Failed to send 500 Internal Server Error response: {}",
                     peer_addr,
                     thread::current().id(),
                     send_err
                 );
                 format!("Failed to send 500 error response: {}", send_err)
             })?; // ? operator propagates error if sending fails.
            return Ok(()); // Return Ok because we've handled error by sending a response. ✅
        }
    };
    let filename_os = path
        .file_name()
        .ok_or_else(|| format!("No filename for path {:?}", path))?; // Get filename from path.
    let filename = filename_os.to_string_lossy(); // Convert filename to a String (lossy if needed).

    // Check for "Range" header to handle partial content requests. 🔑
    let range_header_value = headers.get("Range");

    let mut start_byte = 0; // Default start byte of the file.
    let mut end_byte = file_size - 1; // Default end byte of the file (entire file).

    let response: String;
    if let Some(range_header) = range_header_value {
        // If a "Range" header is present, parse it. 🔍
        let range_bytes_prefix = "bytes=";
        if range_header.starts_with(range_bytes_prefix) {
            // Range header must start with "bytes=".
            let range_header_trimmed = range_header.trim_start_matches(range_bytes_prefix); // Remove "bytes=".
            let range_parts: Vec<&str> = range_header_trimmed.split('-').collect(); // Split range into start and end parts.
            if range_parts.len() == 2 {
                // Range must have two parts (start-end).
                if let Ok(start) = u64::from_str(range_parts[0]) {
                    // Parse the start byte.
                    start_byte = start; // Set the start byte.
                    if range_parts[1].is_empty() {
                        // If end byte is empty, set it to the last byte of file.
                        end_byte = file_size - 1;
                    } else if let Ok(end) = u64::from_str(range_parts[1]) {
                        // Parse the end byte if provided.
                        end_byte = end; // Set the end byte.
                    } else {
                        // If end byte parsing fails, it's a bad request. ❌
                        warn!(
                            "[Peer: {}][ThreadId({:?})] Invalid end byte in Range header: '{}'",
                            peer_addr,
                            thread::current().id(),
                            range_header
                        ); // Log warning about invalid range end. ⚠️
                        send_response(
                             stream,
                             400,
                             "Bad Request",
                             "Invalid Range header",
                             peer_addr,
                         )
                         .map_err(|send_err| {
                             // Send 400 error response. If sending fails, log error. 🚨
                             error!(
                                 "[Peer: {}][ThreadId({:?})] Failed to send 400 Bad Request response for invalid range end: {}",
                                 peer_addr,
                                 thread::current().id(),
                                 send_err
                             );
                             format!("Failed to send 400 error response: {}", send_err)
                         })?; // ? operator propagates error if sending fails.
                        return Ok(()); // Return Ok because we've handled error by sending response. ✅
                    }
                } else {
                    // If start byte parsing fails, it's a bad request. ❌
                    warn!(
                        "[Peer: {}][ThreadId({:?})] Invalid start byte in Range header: '{}'",
                        peer_addr,
                        thread::current().id(),
                        range_header
                    ); // Log warning about invalid range start. ⚠️
                    send_response(
                         stream,
                         400,
                         "Bad Request",
                         "Invalid Range header",
                         peer_addr,
                     )
                     .map_err(|send_err| {
                         // Send 400 error response. If sending fails, log error. 🚨
                         error!(
                             "[Peer: {}][ThreadId({:?})] Failed to send 400 Bad Request response for invalid range start: {}",
                             peer_addr,
                             thread::current().id(),
                             send_err
                         );
                         format!("Failed to send 400 error response: {}", send_err)
                     })?; // ? operator propagates error if sending fails.
                    return Ok(()); // Return Ok because we've handled error by sending response. ✅
                }
            } else {
                // If range format is incorrect (not two parts), it's a bad request. ❌
                warn!(
                    "[Peer: {}][ThreadId({:?})] Invalid Range header format: '{}'",
                    peer_addr,
                    thread::current().id(),
                    range_header
                ); // Log warning about invalid range format. ⚠️
                send_response(
                     stream,
                     400,
                     "Bad Request",
                     "Invalid Range header format",
                     peer_addr,
                 )
                 .map_err(|send_err| {
                     // Send 400 error response. If sending fails, log error. 🚨
                     error!(
                         "[Peer: {}][ThreadId({:?})] Failed to send 400 Bad Request response for range format: {}",
                         peer_addr,
                         thread::current().id(),
                         send_err
                     );
                     format!("Failed to send 400 error response: {}", send_err)
                 })?; // ? operator propagates error if sending fails.
                return Ok(()); // Return Ok because we've handled error by sending response. ✅
            }
        } else {
            // If range header doesn't start with "bytes=", it's a bad request. ❌
            warn!(
                 "[Peer: {}][ThreadId({:?})] Invalid Range header format (missing bytes= prefix): '{}'",
                 peer_addr,
                 thread::current().id(),
                 range_header
             ); // Log warning about missing "bytes=" prefix. ⚠️
            send_response(
                 stream,
                 400,
                 "Bad Request",
                 "Invalid Range header format",
                 peer_addr,
             )
             .map_err(|send_err| {
                 // Send 400 error response. If sending fails, log error. 🚨
                 error!(
                     "[Peer: {}][ThreadId({:?})] Failed to send 400 Bad Request response for range prefix: {}",
                     peer_addr,
                     thread::current().id(),
                     send_err
                 );
                 format!("Failed to send 400 error response: {}", send_err)
             })?; // ? operator propagates error if sending fails.
            return Ok(()); // Return Ok because we've handled error by sending response. ✅
        }

        // Construct the 206 Partial Content response for range requests. 📦
        response = format!(
             "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {}-{}/{}\r\nContent-Disposition: attachment; filename=\"{}\"\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n Accept-Ranges: bytes\r\n\r\n",
             start_byte, end_byte, file_size, filename, end_byte - start_byte + 1
         );
        info!(
             "[Peer: {}][ThreadId({:?})] 206 Partial Content response prepared for '{}', Range: bytes={}-{}/{}",
             peer_addr,
             thread::current().id(),
             path.display(),
             start_byte,
             end_byte,
             file_size
         ); // Log 206 response preparation at info level. ℹ️
    } else {
        // If no "Range" header, serve the full file with 200 OK. 🎉
        response = format!(
             "HTTP/1.1 200 OK\r\nContent-Disposition: attachment; filename=\"{}\"\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\n Accept-Ranges: bytes\r\n\r\n",
             filename, file_size
         );
        info!(
            "[Peer: {}][ThreadId({:?})] 200 OK response prepared for '{}', Content-Length: {}",
            peer_addr,
            thread::current().id(),
            path.display(),
            file_size
        ); // Log 200 response preparation at info level. ℹ️
    }

    // Send the HTTP response headers to the client. 📤
    if let Err(e) = stream.write_all(response.as_bytes()) {
        // If writing headers fails, handle the error. 🙁
        if e.kind() == ErrorKind::BrokenPipe {
            // Broken pipe usually means client disconnected, which is okay to handle gracefully. 😌
            debug!(
                 "[Peer: {}][ThreadId({:?})] Client disconnected (BrokenPipe) during header write for '{}'",
                 peer_addr,
                 thread::current().id(),
                 path.display()
             ); // Log client disconnection at debug level. ⚙️
            return Ok(()); // Gracefully return Ok for broken pipe. ✅
        }
        error!(
            "[Peer: {}][ThreadId({:?})] Error writing header for '{}': {}",
            peer_addr,
            thread::current().id(),
            path.display(),
            e
        ); // Log header writing error. 🚨
        return Err(e.into()); // Convert IO error to boxed error and return.
    }

    // Check if the requested range is satisfiable. 🔍
    if start_byte >= file_size {
        // If start byte is beyond file size, range is not satisfiable (416 error). ❌
        warn!(
             "[Peer: {}][ThreadId({:?})] Requested range not satisfiable for '{}'. Start byte: {}, File size: {}",
             peer_addr,
             thread::current().id(),
             path.display(),
             start_byte,
             file_size
         ); // Log warning about unsatisfiable range. ⚠️
        send_response(
            stream,
            416,
            "Requested Range Not Satisfiable",
            "Range not satisfiable",
            peer_addr,
        )
        .map_err(|send_err| {
            // Send 416 error response. If sending fails, log error. 🚨
            error!(
                "[Peer: {}][ThreadId({:?})] Failed to send 416 Range Not Satisfiable response: {}",
                peer_addr,
                thread::current().id(),
                send_err
            );
            format!("Failed to send 416 error response: {}", send_err)
        })?; // ? operator propagates error if sending fails.
        return Ok(()); // Return Ok because we've handled error by sending response. ✅
    }

    // Seek to the starting byte of the requested range in the file. 🧲
    let seek_result = file.seek(SeekFrom::Start(start_byte));
    if let Err(e) = seek_result {
        // If seeking fails, handle the error. 🙁
        error!(
            "[Peer: {}][ThreadId({:?})] Failed to seek file '{}' to byte {}: {}",
            peer_addr,
            thread::current().id(),
            path.display(),
            start_byte,
            e
        ); // Log file seeking error. 🚨
        send_response(
             stream,
             500,
             "Internal Server Error",
             "Failed to seek file",
             peer_addr,
         )
         .map_err(|send_err| {
             // Send 500 error response. If sending fails, log error. 🚨
             error!(
                 "[Peer: {}][ThreadId({:?})] Failed to send 500 Internal Server Error response after seek failure: {}",
                 peer_addr,
                 thread::current().id(),
                 send_err
             );
             format!("Failed to send 500 error response: {}", send_err)
         })?; // ? operator propagates error if sending fails.
        return Ok(()); // Return Ok because we've handled error by sending response. ✅
    }

    let mut bytes_remaining = (end_byte - start_byte + 1) as usize; // Calculate remaining bytes to send.
    let mut read_buffer = vec![0; chunk_size]; // Create a buffer to read file chunks into.

    // Start sending file data in chunks. 📦
    while bytes_remaining > 0 {
        // Loop until all bytes in the range are sent. 🔄
        let bytes_to_read = std::cmp::min(bytes_remaining, chunk_size); // Determine how many bytes to read in this chunk (up to chunk_size).
        let bytes_read_res = file.read(&mut read_buffer[..bytes_to_read]); // Read a chunk of data from the file.
        match bytes_read_res {
            Ok(bytes_read) => {
                // If reading from file is successful. 🎉
                if bytes_read == 0 {
                    // If we read 0 bytes, it means end of file. 🏁
                    debug!(
                        "[Peer: {}][ThreadId({:?})] End of file '{}' reached.",
                        peer_addr,
                        thread::current().id(),
                        path.display()
                    ); // Log end of file at debug level. ⚙️
                    break; // Exit the loop as file sending is complete.
                }
                let write_result = stream.write_all(&read_buffer[..bytes_read]); // Send the read chunk to the client.
                match write_result {
                    Ok(_) => {} // If writing to stream is successful, continue to next chunk. 🎉
                    Err(e) => {
                        // If writing to stream fails, handle the error. 🙁
                        if e.kind() == ErrorKind::BrokenPipe {
                            // Broken pipe likely means client disconnected. Handle gracefully. 😌
                            debug!(
                                 "[Peer: {}][ThreadId({:?})] Client disconnected (BrokenPipe) during file transfer for '{}'",
                                 peer_addr,
                                 thread::current().id(),
                                 path.display()
                             ); // Log client disconnection at debug level. ⚙️
                            return Ok(()); // Gracefully return Ok for broken pipe. ✅
                        } else {
                            // For other write errors, log error and try to shutdown connection. 🚨
                            error!(
                                "[Peer: {}][ThreadId({:?})] Error writing data chunk for '{}': {}",
                                peer_addr,
                                thread::current().id(),
                                path.display(),
                                e
                            ); // Log data chunk write error. 🚨
                            warn!(
                                 "[Peer: {}][ThreadId({:?})] Attempting to shutdown and close stream due to write error.",
                                 peer_addr,
                                 thread::current().id()
                             ); // Log attempt to shutdown stream at warning level. ⚠️
                            if let Err(shutdown_err) = stream.shutdown(Shutdown::Both) {
                                // Try to shutdown both read and write sides of the stream. If fails, log error. 🚨
                                error!(
                                    "[Peer: {}][ThreadId({:?})] Error shutting down stream: {}",
                                    peer_addr,
                                    thread::current().id(),
                                    shutdown_err
                                ); // Log stream shutdown error. 🚨
                            }
                            if let Err(close_err) = stream.take_error() {
                                // Try to get and log any closing errors. 🚨
                                error!(
                                     "[Peer: {}][ThreadId({:?})] Error closing stream (take_error): {}",
                                     peer_addr,
                                     thread::current().id(),
                                     close_err
                                 ); // Log stream close error. 🚨
                            }
                            return Ok(()); // Return Ok after handling write error and stream shutdown. ✅
                        }
                    }
                }
                bytes_remaining -= bytes_read; // Decrease remaining bytes by the number of bytes read in this chunk.
            }
            Err(e) => {
                // If reading from file fails, log error and return. 🚨
                error!(
                    "[Peer: {}][ThreadId({:?})] Error reading file '{}': {}",
                    peer_addr,
                    thread::current().id(),
                    path.display(),
                    e
                ); // Log file reading error. 🚨
                return Ok(()); // Return Ok after handling file read error. ✅
            }
        }
    }
    info!(
        "[Peer: {}][ThreadId({:?})] serve_file finished for: '{}'",
        peer_addr,
        thread::current().id(),
        path.display()
    ); // Log file serving completion at info level. ℹ️
    Ok(()) // Return Ok to indicate successful file serving. ✅
}

// Serves a directory listing as an HTML page. 📂
fn serve_directory(
    stream: &mut TcpStream,
    path: PathBuf,
    peer_addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "[Peer: {}][ThreadId({:?})] serve_directory started for: '{}'",
        peer_addr,
        thread::current().id(),
        path.display()
    ); // Log directory serving start at info level. ℹ️

    // Generate HTML for directory listing. 🌐
    let html_result = generate_directory_listing(&path);
    match html_result {
        Ok(html) => {
            // If HTML generation is successful, send it as a 200 OK response. 🎉
            send_response(stream, 200, "OK", &html, peer_addr).map_err(|e| {
                 // If sending response fails, log error. 🚨
                 error!(
                     "[Peer: {}][ThreadId({:?})] Failed to send 200 OK directory listing response: {}",
                     peer_addr,
                     thread::current().id(),
                     e
                 );
                 format!("Failed to send 200 OK response: {}", e)
             })?; // ? operator propagates error if sending response fails.
        }
        Err(e) => {
            // If HTML generation fails, send a 500 Internal Server Error response. ❌
            error!(
                "[Peer: {}][ThreadId({:?})] Error generating directory listing for '{}': {}",
                peer_addr,
                thread::current().id(),
                path.display(),
                e
            ); // Log directory listing generation error. 🚨
            send_response(
                 stream,
                 500,
                 "Internal Server Error",
                 "Failed to generate directory listing",
                 peer_addr,
             )
             .map_err(|send_err| {
                 // Send 500 error response. If sending fails, log error. 🚨
                 error!(
                     "[Peer: {}][ThreadId({:?})] Failed to send 500 Internal Server Error response for directory listing: {}",
                     peer_addr,
                     thread::current().id(),
                     send_err
                 );
                 format!("Failed to send 500 error response: {}", send_err)
             })?; // ? operator propagates error if sending fails.
        }
    }
    info!(
        "[Peer: {}][ThreadId({:?})] serve_directory finished for: '{}'",
        peer_addr,
        thread::current().id(),
        path.display()
    ); // Log directory serving completion at info level. ℹ️
    Ok(()) // Return Ok to indicate successful directory serving. ✅
}

// Generates an HTML directory listing for a given path. 🌐
fn generate_directory_listing(path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    debug!(
        "[ThreadId({:?})] Generating directory listing for: '{}'",
        thread::current().id(),
        path.display()
    ); // Log directory listing generation start at debug level. ⚙️

    // Read the directory entries. 📂
    let read_dir_result = fs::read_dir(path);
    let entries_iter = match read_dir_result {
        Ok(rd) => rd, // If reading directory succeeds, get the directory entries iterator. 🎉
        Err(e) => {
            // If reading directory fails, handle the error. 🙁
            let err_msg = format!("Failed to read directory '{}': {}", path.display(), e);
            error!("[ThreadId({:?})] {}", thread::current().id(), err_msg); // Log directory reading error. 🚨
            return Err(From::from(err_msg)); // Return error as boxed error.
        }
    };

    let mut entries: Vec<PathBuf> = Vec::new(); // Initialize a vector to store directory entries as PathBufs.

    // Iterate through directory entries. 🚶
    for entry_result in entries_iter {
        match entry_result {
            Ok(entry) => {
                // If reading a directory entry is successful. 🎉
                entries.push(entry.path()); // Add the path of the entry to the vector.
            }
            Err(e) => {
                // If reading a directory entry fails, log a warning and skip. ⚠️
                warn!(
                    "[ThreadId({:?})] Skipping directory entry due to error: {}",
                    thread::current().id(),
                    e
                ); // Log warning about skipping directory entry. ⚠️
            }
        }
    }
    entries.sort(); // Sort the entries alphabetically. 🗂️

    // Generate breadcrumbs for navigation in the directory listing. 🍞
    let mut _breadcrumbs = String::new();
    let mut current_link = String::from("/"); // Start with the root link.
    for ancestor in path.ancestors().skip(1) {
        // Iterate over path ancestors (parent directories), skipping the path itself. 🚶
        if let Some(name_os) = ancestor.file_name() {
            // For each ancestor, get the filename.
            let name = name_os.to_string_lossy(); // Convert filename to String (lossy).
            _breadcrumbs += &format!(
                r#"<li class="breadcrumb-item"><a href="{link}">{name}</a></li>"#,
                link = current_link, // Link to the current ancestor level.
                name = name          // Display name of the ancestor directory.
            );
            current_link = format!("{}/{}", current_link, name); // Update current link for next ancestor level.
        }
    }
    _breadcrumbs = _breadcrumbs.trim_end_matches('/').to_string(); // Remove trailing slash from breadcrumbs.

    let mut table_rows_html = String::new(); // String to store HTML table rows for directory entries.

    // Generate HTML table rows for each directory entry. 🌐
    for path in entries {
        let row_html_result = generate_directory_row_html(&path); // Generate HTML row for each entry path.
        match row_html_result {
            Ok(row_html) => {
                // If HTML row generation is successful. 🎉
                table_rows_html += &row_html; // Append the row HTML to the table rows string.
            }
            Err(e) => {
                // If HTML row generation fails, log a warning and skip. ⚠️
                warn!(
                    "[ThreadId({:?})] Could not generate table row for path '{}': {}",
                    thread::current().id(),
                    path.display(),
                    e
                ); // Log warning about row generation failure. ⚠️
            }
        }
    }

    // Construct the full HTML page for the directory listing. 🌐
    let html = format!(
        r#"
         <!DOCTYPE html>
         <html lang="en">
         <head>
             <meta charset="UTF-8">
             <meta name="viewport" content="width=device-width, initial-scale=1.0">
             <title>SimpleDownloadServer</title>
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
                     border_collapse: collapse;
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
                 </ul>
                 <h1>Directory Listing</h1>
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
        table_rows_html // Placeholder for table rows HTML.
    );
    debug!(
        "[ThreadId({:?})] Directory listing HTML generated for: '{}'",
        thread::current().id(),
        path.display()
    ); // Log HTML generation completion at debug level. ⚙️
    Ok(html) // Return generated HTML string wrapped in Ok. ✅
}

// Generates a single table row (<tr>) HTML for a directory entry (file or subdirectory). 🌐
fn generate_directory_row_html(path: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
    // Get metadata for the path (file or directory). 🗂️
    let metadata = fs::metadata(path).map_err(|e| {
        // If getting metadata fails, handle error. 🙁
        error!(
            "[ThreadId({:?})] Failed to get metadata for '{}': {}",
            thread::current().id(),
            path.display(),
            e
        ); // Log metadata error. 🚨
        format!("Failed to get metadata for '{}': {}", path.display(), e)
    })?; // ? operator propagates error if metadata retrieval fails.

    // Format file size in human-readable format (e.g., "1.2 MB"). 📊
    let file_size_human = metadata.len().file_size(options::BINARY).map_err(|e| {
        // If formatting file size fails, handle error. 🙁
        error!(
            "[ThreadId({:?})] Failed to format file size for '{}': {}",
            thread::current().id(),
            path.display(),
            e
        ); // Log file size formatting error. 🚨
        format!("Failed to format file size for '{}': {}", path.display(), e)
    })?; // ? operator propagates error if size formatting fails.

    // Get last modification time of the file/directory. ⏰
    let last_modified: SystemTime = metadata.modified().map_err(|e| {
        // If getting modification time fails, handle error. 🙁
        error!(
            "[ThreadId({:?})] Failed to get modification time for '{}': {}",
            thread::current().id(),
            path.display(),
            e
        ); // Log modification time error. 🚨
        format!(
            "Failed to get modification time for '{}': {}",
            path.display(),
            e
        )
    })?; // ? operator propagates error if modification time retrieval fails.

    // Convert SystemTime to DateTime in local timezone and format as string. 📅
    let datetime: DateTime<Local> = DateTime::from(last_modified);
    let last_modified_str = datetime.format("%d-%m-%Y %H:%M:%S").to_string(); // Format date and time.

    // Get the parent directory of the current path. 📂
    let current_dir = path
        .parent()
        .ok_or_else(|| format!("Path '{}' has no parent", path.display()))?;
    // Calculate the relative path from the current directory. 🗺️
    let relative_path = path.strip_prefix(current_dir).map_err(|e| {
        // If stripping prefix fails, handle error. 🙁
        error!(
            "[ThreadId({:?})] Failed to strip prefix from path '{}': {}",
            thread::current().id(),
            path.display(),
            e
        ); // Log path stripping error. 🚨
        format!(
            "Failed to strip prefix from path '{}': {}",
            path.display(),
            e
        )
    })?; // ? operator propagates error if prefix stripping fails.

    let filename_os = path
        .file_name()
        .ok_or_else(|| format!("No filename for path {:?}", path))?; // Get filename from path.
    let filename = filename_os.to_string_lossy(); // Convert filename to String (lossy).

    // Format the HTML table row (<tr>) with a link to the entry, size, and last modified time. 🌐
    Ok(format!(
        "<tr><td><a href=\"{}\">{}</a></td><td>{}</td><td>{}</td></tr>",
        percent_encode_path(relative_path), // URL-encode the relative path for the link.
        filename,                           // Display filename as link text.
        file_size_human,                    // Display human-readable file size in size column.
        last_modified_str // Display formatted last modification time in last modified column.
    ))
}

// Helper function to percent-encode path segments for URLs. 🌐
fn percent_encode_path(path: &Path) -> String {
    path.components() // Iterate over path components. 🚶
        .filter_map(|component| match component {
            // Filter and map path components. 🗺️
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()), // For normal components (filenames/dirnames), convert to String.
            _ => None, // Skip RootDir, ParentDir, CurDir, Prefix components - we don't need to encode these special components.
        })
        .collect::<Vec<_>>() // Collect all String components into a vector.
        .join("/") // Join the components with "/" to form the path string.
        .replace(" ", "%20") // Replace spaces with "%20" for URL encoding - important for spaces in filenames!
}

// Extracts the requested path from the HTTP request line. 🗺️
fn get_request_path(request_line: &str) -> &str {
    // Check if the request line starts with "GET ". 🔍
    if request_line.starts_with("GET ") {
        // Find the first space after "GET " - this marks the start of the path.
        if let Some(path_start_index) = request_line.find(' ') {
            // Get the part of the request line after "GET ".
            let path_with_http_version = &request_line[path_start_index + 1..];
            // Find the next space - this marks the end of the path (before HTTP version).
            if let Some(path_end_index) = path_with_http_version.find(' ') {
                // Extract the path part.
                let path = &path_with_http_version[..path_end_index];
                // Handle paths that start with "/".
                if path.starts_with("/") {
                    let relative_path = &path[1..]; // Remove the leading "/".
                    if relative_path.is_empty() {
                        // If it's just "/", return root path.
                        return "/";
                    } else {
                        // Otherwise, return the relative path.
                        return relative_path;
                    }
                } else {
                    // If it doesn't start with "/", return the path as is.
                    return path;
                }
            } else {
                // If there's no second space (unusual HTTP request but handle it).
                let path = path_with_http_version; // Take the rest as path.
                                                   // Handle paths starting with "/".
                if path.starts_with("/") {
                    let relative_path = &path[1..]; // Remove leading "/".
                    if relative_path.is_empty() {
                        // If it's just "/", return root path.
                        return "/";
                    } else {
                        // Otherwise return the relative path.
                        return relative_path;
                    }
                } else {
                    // If it doesn't start with "/", return the path as is.
                    return path;
                }
            }
        }
    }
    "/" // Default to root path if request line parsing fails - safer fallback. 🗺️
}

// Sends an HTTP response to the client, including headers and body (can be HTML or error image). 📤
fn send_response(
    stream: &mut TcpStream,
    status_code: u16,
    status_text: &str,
    body: &str,
    peer_addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    debug!(
        "[Peer: {}][ThreadId({:?})] send_response - Status: {}, Status Text: {}, Body Length: {}",
        peer_addr,
        thread::current().id(),
        status_code,
        status_text,
        body.len()
    ); // Log response sending attempt at debug level. ⚙️

    // Map status codes to embedded error image filenames. 🖼️
    let image_map = [
        (400, "error_400.dat"),
        (403, "error_403.dat"),
        (404, "error_404.dat"),
    ];

    let (content_type, response_body) = if let Some((_, image_name)) =
        image_map.iter().find(|(code, _)| *code == status_code)
    {
        // If status code is in the image map, try to get the embedded image. 🖼️
        match Assets::get(image_name) {
            Some(embedded_file) => ("image/png", embedded_file.data.into_owned()), // If image found, use PNG content type and image data as body. 🎉
            None => {
                // If embedded image not found, log a warning and serve a default text error. ⚠️
                warn!(
                         "[Peer: {}][ThreadId({:?})] Embedded image '{}' for status code {} not found. Serving default text error.",
                         peer_addr,
                         thread::current().id(),
                         image_name,
                         status_code
                     ); // Log warning about missing image. ⚠️
                (
                    "text/plain",
                    format!("Error {}: {}. Image not found.", status_code, status_text)
                        .as_bytes()
                        .to_vec(), // Use plain text content type and a text error message.
                )
            }
        }
    } else {
        // For other status codes (e.g., 200, 206), serve HTML body. 🌐
        ("text/html; charset=utf-8", body.as_bytes().to_vec()) // Use HTML content type and the provided HTML body.
    };

    // Format the HTTP response with status line, headers, and an empty line before the body. ✉️
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
        status_code,         // HTTP status code (e.g., 200, 404, 500).
        status_text,         // HTTP status text (e.g., "OK", "Not Found", "Internal Server Error").
        content_type,        // Content-Type header (e.g., "text/html", "image/png").
        response_body.len()  // Content-Length header - length of the response body in bytes.
    );

    // Send the response headers to the client. 📤
    stream.write_all(response.as_bytes()).map_err(|e| {
         // If writing headers fails, log error. 🚨
         error!(
             "[Peer: {}][ThreadId({:?})] Failed to write response header (Status: {}, Content-Type: {}): {}",
             peer_addr,
             thread::current().id(),
             status_code,
             content_type,
             e
         );
         format!("Failed to write response header: {}", e)
     })?; // ? operator propagates error if header writing fails.

    // Send the response body (HTML or image data) to the client. 📤
    stream.write_all(&response_body).map_err(|e| {
         // If writing body fails, log error. 🚨
         error!(
             "[Peer: {}][ThreadId({:?})] Failed to write response body (Status: {}, Content-Type: {}, Body Length: {}): {}",
             peer_addr,
             thread::current().id(),
             status_code,
             content_type,
             response_body.len(),
             e
         );
         format!("Failed to write response body: {}", e)
     })?; // ? operator propagates error if body writing fails.
    Ok(()) // Return Ok to indicate successful response sending. ✅
}
