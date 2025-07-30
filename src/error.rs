use std::fmt;

#[derive(Debug)]
pub enum AppError {
    Io(std::io::Error),
    Glob(glob::PatternError),
    AddrParse(std::net::AddrParseError),
    InvalidPath,
    DirectoryNotFound(String),
    Forbidden,
    NotFound,
    BadRequest,
    Unauthorized,
    MethodNotAllowed,
    InternalServerError(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(err) => write!(f, "IO error: {err}"),
            AppError::Glob(err) => write!(f, "Glob pattern error: {err}"),
            AppError::AddrParse(err) => write!(f, "Address parse error: {err}"),
            AppError::InvalidPath => write!(f, "Invalid path"),
            AppError::DirectoryNotFound(path) => write!(f, "Directory not found: {path}"),
            AppError::Forbidden => write!(f, "Forbidden"),
            AppError::NotFound => write!(f, "Not Found"),
            AppError::BadRequest => write!(f, "Bad request"),
            AppError::Unauthorized => write!(f, "Unauthorized"),
            AppError::MethodNotAllowed => write!(f, "Method not allowed"),
            AppError::InternalServerError(msg) => write!(f, "Internal server error: {msg}"),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err)
    }
}

impl From<glob::PatternError> for AppError {
    fn from(err: glob::PatternError) -> Self {
        AppError::Glob(err)
    }
}

impl From<std::net::AddrParseError> for AppError {
    fn from(err: std::net::AddrParseError) -> Self {
        AppError::AddrParse(err)
    }
}

impl std::error::Error for AppError {}
