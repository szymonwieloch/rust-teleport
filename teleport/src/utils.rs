use std::env::current_exe;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use uuid::Uuid;

/// Returns the directory containing the current executable.
pub fn exe_dir() -> Result<PathBuf, io::Error> {
    let mut path_buf = current_exe()?;
    if !path_buf.pop() {
        return Err(io::Error::other("Executable path is invalid"));
    }
    Ok(path_buf)
}

/// Opens a configuration file.
///
/// If `path` is `Some`, uses that path directly. Otherwise looks for
/// `default_file_name` next to the executable.
///
/// # Panics
///
/// Panics if the configuration file cannot be opened — configuration is
/// considered essential for the application to start.
pub fn open_cfg_file(path: &Option<String>, default_file_name: &str) -> File {
    let cfg_path = match path {
        Some(path) => PathBuf::from(path),
        None => {
            let mut path_buf = exe_dir().expect("Could not obtain executable directory");
            path_buf.push(default_file_name);
            path_buf
        }
    };
    File::open(&cfg_path).unwrap_or_else(|e| {
        panic!("Could not open configuration file {:?}: {}", cfg_path, e);
    })
}

/// Parse a UUID string from a `TaskId` protobuf message.
pub fn parse_uuid_str(uuid_str: &str) -> Result<Uuid, String> {
    Uuid::parse_str(uuid_str).map_err(|e| format!("Invalid UUID format: {}", e))
}
