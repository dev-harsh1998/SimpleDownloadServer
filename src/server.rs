use crate::cli::Cli;
use crate::error::AppError;
use crate::http::handle_client;
use glob::Pattern;
use log::{error, info, warn};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Rate limiter for basic DoS protection
#[derive(Clone)]
pub struct RateLimiter {
    connections: Arc<Mutex<HashMap<IpAddr, ConnectionInfo>>>,
    max_requests_per_minute: u32,
    max_concurrent_per_ip: u32,
}

#[derive(Debug)]
struct ConnectionInfo {
    request_count: u32,
    last_reset: Instant,
    active_connections: u32,
}

impl RateLimiter {
    pub fn new(max_requests_per_minute: u32, max_concurrent_per_ip: u32) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            max_requests_per_minute,
            max_concurrent_per_ip,
        }
    }

    pub fn check_rate_limit(&self, ip: IpAddr) -> bool {
        let mut connections = self.connections.lock().unwrap();
        let now = Instant::now();

        let conn_info = connections.entry(ip).or_insert(ConnectionInfo {
            request_count: 0,
            last_reset: now,
            active_connections: 0,
        });

        // Reset counter if more than a minute has passed
        if now.duration_since(conn_info.last_reset) >= Duration::from_secs(60) {
            conn_info.request_count = 0;
            conn_info.last_reset = now;
        }

        // Check concurrent connections
        if conn_info.active_connections >= self.max_concurrent_per_ip {
            warn!("Rate limit exceeded for {ip}: too many concurrent connections");
            return false;
        }

        // Check request rate
        if conn_info.request_count >= self.max_requests_per_minute {
            warn!("Rate limit exceeded for {ip}: too many requests per minute");
            return false;
        }

        conn_info.request_count += 1;
        conn_info.active_connections += 1;
        true
    }

    pub fn release_connection(&self, ip: IpAddr) {
        if let Ok(mut connections) = self.connections.lock() {
            if let Some(conn_info) = connections.get_mut(&ip) {
                conn_info.active_connections = conn_info.active_connections.saturating_sub(1);
            }
        }
    }

    pub fn cleanup_old_entries(&self) {
        let mut connections = self.connections.lock().unwrap();
        let now = Instant::now();

        connections.retain(|_, info| {
            now.duration_since(info.last_reset) < Duration::from_secs(300) // Keep for 5 minutes
        });
    }
}

/// Server statistics and monitoring
#[derive(Default, Clone)]
pub struct ServerStats {
    pub total_requests: Arc<Mutex<u64>>,
    pub successful_requests: Arc<Mutex<u64>>,
    pub error_requests: Arc<Mutex<u64>>,
    pub bytes_served: Arc<Mutex<u64>>,
    pub start_time: Arc<Mutex<Option<Instant>>>,
}

impl ServerStats {
    pub fn new() -> Self {
        Self {
            total_requests: Arc::new(Mutex::new(0)),
            successful_requests: Arc::new(Mutex::new(0)),
            error_requests: Arc::new(Mutex::new(0)),
            bytes_served: Arc::new(Mutex::new(0)),
            start_time: Arc::new(Mutex::new(Some(Instant::now()))),
        }
    }

    pub fn record_request(&self, success: bool, bytes: u64) {
        if let Ok(mut total) = self.total_requests.lock() {
            *total += 1;
        }

        if success {
            if let Ok(mut successful) = self.successful_requests.lock() {
                *successful += 1;
            }
        } else if let Ok(mut errors) = self.error_requests.lock() {
            *errors += 1;
        }

        if let Ok(mut total_bytes) = self.bytes_served.lock() {
            *total_bytes += bytes;
        }
    }

    pub fn get_stats(&self) -> (u64, u64, u64, u64, Duration) {
        let total = *self
            .total_requests
            .lock()
            .unwrap_or_else(|_| panic!("Stats lock poisoned"));
        let successful = *self
            .successful_requests
            .lock()
            .unwrap_or_else(|_| panic!("Stats lock poisoned"));
        let errors = *self
            .error_requests
            .lock()
            .unwrap_or_else(|_| panic!("Stats lock poisoned"));
        let bytes = *self
            .bytes_served
            .lock()
            .unwrap_or_else(|_| panic!("Stats lock poisoned"));
        let uptime = self
            .start_time
            .lock()
            .unwrap_or_else(|_| panic!("Stats lock poisoned"))
            .map(|start| start.elapsed())
            .unwrap_or_default();

        (total, successful, errors, bytes, uptime)
    }
}

/// Simple native thread pool implementation
pub struct ThreadPool {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>,
}

type Job = Box<dyn FnOnce() + Send + 'static>;

impl ThreadPool {
    pub fn new(size: usize) -> ThreadPool {
        assert!(size > 0);

        let (sender, receiver) = mpsc::channel();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }

        ThreadPool {
            workers,
            sender: Some(sender),
        }
    }

    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let job = Box::new(f);

        if let Some(ref sender) = self.sender {
            if sender.send(job).is_err() {
                warn!("Failed to send job to thread pool");
            }
        }
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        drop(self.sender.take());

        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                if thread.join().is_err() {
                    warn!("Worker thread {} panicked", worker.id);
                }
            }
        }
    }
}

struct Worker {
    id: usize,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<mpsc::Receiver<Job>>>) -> Worker {
        let thread = thread::spawn(move || loop {
            let message = receiver.lock().unwrap().recv();

            match message {
                Ok(job) => {
                    job();
                }
                Err(_) => {
                    break;
                }
            }
        });

        Worker {
            id,
            thread: Some(thread),
        }
    }
}

pub fn run_server(
    cli: Cli,
    shutdown_rx: Option<mpsc::Receiver<()>>,
    addr_tx: Option<mpsc::Sender<SocketAddr>>,
) -> Result<(), AppError> {
    let base_dir = Arc::new(cli.directory.canonicalize()?);

    if !base_dir.is_dir() {
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

    // Initialize security and monitoring systems
    let rate_limiter = Arc::new(RateLimiter::new(120, 10)); // 120 req/min, 10 concurrent per IP
    let stats = Arc::new(ServerStats::new());

    if let Some(tx) = addr_tx {
        if tx.send(local_addr).is_err() {
            return Err(AppError::InternalServerError(
                "Failed to send server address to test thread".to_string(),
            ));
        }
    }

    info!(
        "🚀 Server listening on {} for directory '{}' (allowed extensions: {:?})",
        local_addr,
        base_dir.display(),
        allowed_extensions
    );
    info!("⚡ Security: Rate limiting enabled (120 req/min, 10 concurrent per IP)");
    info!("📊 Monitoring: Statistics collection enabled");

    let pool = ThreadPool::new(cli.threads);
    let username = Arc::new(cli.username);
    let password = Arc::new(cli.password);

    // Start background cleanup task for rate limiter
    let rate_limiter_cleanup = rate_limiter.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(300)); // Cleanup every 5 minutes
            rate_limiter_cleanup.cleanup_old_entries();
        }
    });

    // Start background stats reporting
    let stats_reporter = stats.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(300)); // Report every 5 minutes
            let (total, successful, errors, bytes, uptime) = stats_reporter.get_stats();
            info!(
                "📊 Stats: {} total requests ({} successful, {} errors), {:.2} MB served, uptime: {}s",
                total,
                successful,
                errors,
                bytes as f64 / 1024.0 / 1024.0,
                uptime.as_secs()
            );
        }
    });

    'server_loop: loop {
        if let Some(ref rx) = shutdown_rx {
            if rx.try_recv().is_ok() {
                info!("🛑 Shutdown signal received. Shutting down gracefully.");
                break 'server_loop;
            }
        }

        match listener.accept() {
            Ok((stream, peer_addr)) => {
                let client_ip = peer_addr.ip();

                // Check rate limits
                if !rate_limiter.check_rate_limit(client_ip) {
                    warn!("🚫 Connection from {client_ip} rejected due to rate limiting");
                    drop(stream); // Close connection immediately
                    continue;
                }

                // Ensure the accepted stream is in blocking mode
                if let Err(e) = stream.set_nonblocking(false) {
                    error!("Failed to set stream to blocking mode: {e}");
                    rate_limiter.release_connection(client_ip);
                    continue;
                }

                let (
                    base_dir,
                    allowed_extensions,
                    username,
                    password,
                    chunk_size,
                    rate_limiter,
                    stats,
                ) = (
                    base_dir.clone(),
                    allowed_extensions.clone(),
                    username.clone(),
                    password.clone(),
                    cli.chunk_size,
                    rate_limiter.clone(),
                    stats.clone(),
                );

                pool.execute(move || {
                    let result = handle_client_with_stats(
                        stream,
                        peer_addr,
                        &base_dir,
                        &allowed_extensions,
                        &username,
                        &password,
                        chunk_size,
                        &stats,
                    );

                    // Release rate limit connection
                    rate_limiter.release_connection(client_ip);

                    // Log any errors
                    if let Err(e) = result {
                        warn!("⚠️  Client handling error: {e}");
                    }
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => {
                error!("❌ Error accepting connection: {e}");
            }
        }
    }

    // Final stats report
    let (total, successful, errors, bytes, uptime) = stats.get_stats();
    info!(
        "📊 Final stats: {} total requests ({} successful, {} errors), {:.2} MB served, uptime: {}s",
        total,
        successful,
        errors,
        bytes as f64 / 1024.0 / 1024.0,
        uptime.as_secs()
    );

    info!("✅ Server shut down gracefully.");
    Ok(())
}

/// Enhanced client handler with statistics tracking
#[allow(clippy::too_many_arguments)]
fn handle_client_with_stats(
    stream: std::net::TcpStream,
    peer_addr: SocketAddr,
    base_dir: &Arc<std::path::PathBuf>,
    allowed_extensions: &Arc<Vec<glob::Pattern>>,
    username: &Arc<Option<String>>,
    password: &Arc<Option<String>>,
    chunk_size: usize,
    stats: &ServerStats,
) -> Result<(), AppError> {
    let start = Instant::now();
    let bytes_sent = 0u64;

    // Use existing handle_client but with error tracking
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        handle_client(
            stream,
            base_dir,
            allowed_extensions,
            username,
            password,
            chunk_size,
        );
    }));

    let success = result.is_ok();
    let processing_time = start.elapsed();

    // Record statistics
    stats.record_request(success, bytes_sent);

    if processing_time > Duration::from_millis(1000) {
        warn!(
            "⏱️  Slow request from {}: {}ms",
            peer_addr.ip(),
            processing_time.as_millis()
        );
    }

    if result.is_err() {
        return Err(AppError::InternalServerError(
            "Client handler panicked".to_string(),
        ));
    }

    Ok(())
}
