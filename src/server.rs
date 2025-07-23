use crate::cli::Cli;
use crate::error::AppError;
use crate::fs::is_directory;
use crate::http::handle_client;
use glob::Pattern;
use log::{debug, error, info};
use rand::Rng;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use threadpool::ThreadPool;

/// Runs the main server loop.
///
/// This function sets up the server environment, binds to the specified address,
/// and listens for incoming connections, dispatching them to a thread pool.
pub fn run_server(cli: Cli) -> Result<(), AppError> {
    let file_directory = Arc::new(Mutex::new(
        PathBuf::from(cli.directory.clone()).canonicalize()?,
    ));

    if !is_directory(&file_directory)? {
        return Err(AppError::DirectoryNotFound(
            cli.directory.to_string_lossy().into_owned(),
        ));
    }

    let allowed_extensions = Arc::new(
        cli.allowed_extensions
            .split(',')
            .map(|ext| Pattern::new(ext.trim()))
            .collect::<Result<Vec<Pattern>, _>>()?,
    );

    let bind_address = format!("{}:{}", cli.listen, cli.port);
    let listener = TcpListener::bind(&bind_address)?;

    info!(
        "Server listening on {} for directory '{}' (allowed extensions: {:?})",
        bind_address,
        file_directory.lock().unwrap().display(),
        allowed_extensions
    );

    let pool = ThreadPool::new(cli.threads);
    let base_dir = Arc::new(cli.directory.canonicalize()?);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let file_directory_arc = Arc::clone(&file_directory);
                let allowed_extensions_arc = Arc::clone(&allowed_extensions);
                let chunk_size = cli.chunk_size;
                let base_dir_clone = Arc::clone(&base_dir);

                let peer_addr = stream.peer_addr().map(|s| s.to_string()).unwrap_or_else(|_| "unknown".to_string());
                let request_id = generate_request_id();
                let log_prefix = format!("[ReqID: {}][Peer: {}]", request_id, peer_addr);

                pool.execute(move || {
                    debug!("{} Handling client connection", log_prefix);
                    if let Err(e) = handle_client(
                        stream,
                        &file_directory_arc,
                        &allowed_extensions_arc,
                        chunk_size,
                        &log_prefix,
                        &base_dir_clone,
                    ) {
                        error!("{} Error handling client: {}", log_prefix, e);
                    }
                    debug!("{} Client handled successfully", log_prefix);
                });
            }
            Err(e) => {
                error!("Error accepting connection: {}", e);
            }
        }
    }

    info!("Server shutting down gracefully.");
    Ok(())
}

fn generate_request_id() -> String {
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(8)
        .map(char::from)
        .collect()
}