use crate::cli::Cli;
use crate::error::AppError;
use crate::fs::is_directory;
use crate::http::handle_client;
use glob::Pattern;
use log::{debug, error, info};
use rand::Rng;
use std::net::{SocketAddr, TcpListener};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use threadpool::ThreadPool;

pub fn run_server(
    cli: Cli,
    shutdown_rx: Option<mpsc::Receiver<()>>,
    addr_tx: Option<mpsc::Sender<SocketAddr>>,
) -> Result<(), AppError> {
    let file_directory = Arc::new(Mutex::new(cli.directory.clone().canonicalize()?));

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
    let local_addr = listener.local_addr()?;
    listener.set_nonblocking(true)?;

    if let Some(tx) = addr_tx {
        if tx.send(local_addr).is_err() {
            return Err(AppError::InternalServerError(
                "Failed to send server address to test thread".to_string(),
            ));
        }
    }

    info!(
        "Server listening on {} for directory '{}' (allowed extensions: {:?})",
        local_addr,
        file_directory.lock().unwrap().display(),
        allowed_extensions
    );

    let pool = ThreadPool::new(cli.threads);
    let base_dir = Arc::new(cli.directory.canonicalize()?);
    let username = Arc::new(cli.username);
    let password = Arc::new(cli.password);

    'server_loop: loop {
        if let Some(ref rx) = shutdown_rx {
            if rx.try_recv().is_ok() {
                info!("Shutdown signal received. Shutting down gracefully.");
                break 'server_loop;
            }
        }

        match listener.accept() {
            Ok((stream, _)) => {
                let file_directory_arc = Arc::clone(&file_directory);
                let allowed_extensions_arc = Arc::clone(&allowed_extensions);
                let chunk_size = cli.chunk_size;
                let base_dir_clone = Arc::clone(&base_dir);
                let username_clone = Arc::clone(&username);
                let password_clone = Arc::clone(&password);

                let peer_addr = stream
                    .peer_addr()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|_| "unknown".to_string());
                let request_id = generate_request_id();
                let log_prefix = format!("[ReqID: {request_id}][Peer: {peer_addr}]");

                pool.execute(move || {
                    debug!("{log_prefix} Handling client connection");
                    handle_client(
                        stream,
                        &file_directory_arc,
                        &allowed_extensions_arc,
                        chunk_size,
                        &log_prefix,
                        &base_dir_clone,
                        &username_clone,
                        &password_clone,
                    );
                    debug!("{log_prefix} Client handled successfully");
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => {
                error!("Error accepting connection: {e}");
            }
        }
    }

    info!("Server shutting down gracefully.");
    Ok(())
}

fn generate_request_id() -> String {
    rand::rng()
        .sample_iter(&rand::distr::Alphanumeric)
        .take(8)
        .map(char::from)
        .collect()
}
