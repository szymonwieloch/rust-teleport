use std::env::current_exe;
use std::fs::File;
use std::path::PathBuf;

pub fn exe_dir() -> Result<PathBuf, std::io::Error> {
    let mut path_buf = current_exe()?;
    if !path_buf.pop() {
        return Err(std::io::Error::other(
            "Executable path is invalid",
        ));
    }
    Ok(path_buf)
}

pub fn open_cfg_file(path: &Option<String>, default_file_name: &str) -> File {
    let cfg_path = match path {
        Some(ref path) => PathBuf::from(path),
        None => {
            let mut path_buf = exe_dir().expect("Could not obtain executable directory");
            path_buf.push(default_file_name);
            path_buf
        }
    };
    File::open(cfg_path).expect("Could not open configuration file")
}

/// Wrap text in ANSI green color codes.
#[allow(dead_code)]
pub fn green(text: &str) -> String {
    format!("\x1b[32m{}\x1b[0m", text)
}

/// Wrap text in ANSI red color codes.
#[allow(dead_code)]
pub fn red(text: &str) -> String {
    format!("\x1b[31m{}\x1b[0m", text)
}
