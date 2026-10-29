use std::fmt;

#[derive(Debug)]
pub enum CliError {
    Usage(String),
    Offline(String),
    Timeout(String),
    Core(String),
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Usage(_) => 2,
            CliError::Offline(_) => 3,
            _ => 1,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Usage(s) => write!(f, "{s}"),
            CliError::Offline(s) => write!(f, "{s}"),
            CliError::Timeout(s) => write!(f, "timed out: {s}"),
            CliError::Core(s) => write!(f, "{s}"),
            CliError::Io(e) => write!(f, "{e}"),
            CliError::Json(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        CliError::Io(e)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(e: serde_json::Error) -> Self {
        CliError::Json(e)
    }
}
