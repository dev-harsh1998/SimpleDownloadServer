use crate::error::AppError;
use crate::utils::percent_encode_path;
use chrono::{DateTime, Local};
use humansize::{format_size, BINARY};
use log::{debug, warn};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Checks if the given path is a directory.
pub fn is_directory(file_directory: &Arc<Mutex<PathBuf>>) -> Result<bool, AppError> {
    let dir_guard = file_directory.lock().map_err(|_| {
        AppError::InternalServerError("Failed to lock directory mutex".to_string())
    })?;
    Ok(Path::new(&*dir_guard).is_dir())
}

/// Generates an HTML directory listing for a given path.
pub fn generate_directory_listing(path: &Path, log_prefix: &str) -> Result<String, AppError> {
    debug!("{} Generating directory listing for: '{}'", log_prefix, path.display());

    let mut entries: Vec<PathBuf> = Vec::new();
    for entry_result in fs::read_dir(path)? {
        match entry_result {
            Ok(entry) => entries.push(entry.path()),
            Err(e) => {
                warn!("{} Skipping directory entry due to error: {}", log_prefix, e);
            }
        }
    }
    entries.sort();

    let mut table_rows_html = String::new();
    for path in entries {
        match generate_directory_row_html(&path, log_prefix) {
            Ok(row_html) => table_rows_html.push_str(&row_html),
            Err(e) => {
                warn!(
                    "{} Could not generate table row for path '{}': {}",
                    log_prefix,
                    path.display(),
                    e
                );
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
             <title>SimpleDownloadServer</title>
             <link
                 href="https://stackpath.bootstrapcdn.com/bootstrap/5.3.0/css/bootstrap.min.css"
                 rel="stylesheet"
             >
             <style>
                 body {{
                     font-family: 'Inter', sans-serif;
                     background-color: #1a1a1a;
                     color: #FFFFFF;
                     margin: 0;
                     padding: 20px;
                 }}
                 .container {{
                     max-width: 960px;
                     margin: 0 auto;
                     padding: 30px;
                     background-color: #424242;
                     border-radius: 10px;
                     box-shadow: 0 4px 8px rgba(0, 0, 0, 0.7);
                     transition: box-shadow 0.3s ease-in-out;
                 }}
                 .container:hover {{
                   box-shadow:
                     0px 8px 20px rgba(150, 150, 150, 0.2),
                     0px -8px 20px rgba(150, 150, 150, 0.2),
                     8px 0px 20px rgba(150, 150, 150, 0.2),
                     -8px 0px 20px rgba(150, 150, 150, 0.2);
                 }}
                 h1 {{
                     color: #FF9800;
                     margin-bottom: 30px;
                 }}
                 table {{
                     width: 100%;
                     border-collapse: collapse;
                 }}
                 th, td {{
                     padding: 10px;
                     text-align: left;
                     border-bottom: 1px solid #555555;
                 }}
                 th {{
                     background-color: #616161;
                 }}
                 tr:hover {{
                     background-color: #757575;
                 }}
                 a {{
                      color: white;
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
        table_rows_html
    );
    debug!("{} Directory listing HTML generated for: '{}'", log_prefix, path.display());
    Ok(html)
}

/// Generates a single table row for a directory entry.
pub fn generate_directory_row_html(path: &Path, _log_prefix: &str) -> Result<String, AppError> {
    let metadata = fs::metadata(path)?;
    let file_size_human = format_size(metadata.len(), BINARY);

    let last_modified: SystemTime = metadata.modified()?;
    let datetime: DateTime<Local> = DateTime::from(last_modified);
    let last_modified_str = datetime.format("%d-%m-%Y %H:%M:%S").to_string();

    let filename = path.file_name().unwrap().to_string_lossy();
    let relative_path = percent_encode_path(Path::new(&filename.to_string()));

    Ok(format!(
        "<tr><td><a href=\"{}\">{}</a></td><td>{}</td><td>{}</td></tr>",
        relative_path, filename, file_size_human, last_modified_str
    ))
}
