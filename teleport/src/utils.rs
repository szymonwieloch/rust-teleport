use std::env::current_exe;
use std::path::PathBuf;

pub fn exe_dir() -> Result<PathBuf, std::io::Error> {
    let mut path_buf = current_exe()?;
    if !path_buf.pop() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Executable path is invalid",
        ));
    }
    Ok(path_buf)
}
