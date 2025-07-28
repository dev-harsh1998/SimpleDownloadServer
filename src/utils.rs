use std::path::{Component, Path};

// Helper function to percent-encode path segments for URLs. 🌐
pub fn percent_encode_path(path: &Path) -> String {
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
pub fn get_request_path(request_line: &str) -> &str {
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
                if let Some(relative_path) = path.strip_prefix("/") {
                    // Remove the leading "/".
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
                if let Some(relative_path) = path.strip_prefix("/") {
                    // Remove leading "/".
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
