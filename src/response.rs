use crate::error::AppError;
use log::{debug, error, warn};
use rust_embed::RustEmbed;
use std::io::prelude::*;
use std::net::TcpStream;

#[derive(RustEmbed)]
#[folder = "assets"]
struct Assets;

/// Sends a response to the client.
///
/// This function handles sending various types of responses, including HTML,
/// images for error pages, and plain text fallbacks.
pub fn send_response(
    stream: &mut TcpStream,
    status_code: u16,
    status_text: &str,
    body: &str,
    log_prefix: &str,
) -> Result<(), AppError> {
    debug!(
        "{} send_response - Status: {}, Status Text: {}, Body Length: {}",
        log_prefix,
        status_code,
        status_text,
        body.len()
    );

    let image_map = [
        (400, "error_400.dat"),
        (403, "error_403.dat"),
        (404, "error_404.dat"),
    ];

    let (content_type, response_body) =
        if let Some((_, image_name)) = image_map.iter().find(|(code, _)| *code == status_code) {
            match Assets::get(image_name) {
                Some(embedded_file) => ("image/png", embedded_file.data.into_owned()),
                None => {
                    warn!(
                        "{} Embedded image '{}' for status code {} not found. Serving default text error.",
                        log_prefix, image_name, status_code
                    );
                    (
                        "text/plain",
                        format!("Error {}: {}. Image not found.", status_code, status_text)
                            .as_bytes()
                            .to_vec(),
                    )
                }
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

    stream.write_all(response.as_bytes()).map_err(|e| {
        error!(
            "{} Failed to write response header (Status: {}, Content-Type: {}): {}",
            log_prefix, status_code, content_type, e
        );
        AppError::Io(e)
    })?;

    stream.write_all(&response_body).map_err(|e| {
        error!(
            "{} Failed to write response body (Status: {}, Content-Type: {}, Body Length: {}): {}",
            log_prefix,
            status_code,
            content_type,
            response_body.len(),
            e
        );
        AppError::Io(e)
    })?;

    Ok(())
}