use super::utils::exe_dir;
use clap::Clap;
use serde::Deserialize;
use serde_yaml::from_reader;
use std::fs::File;

use std::path::PathBuf;

#[derive(Clap)]
#[clap(version = "1.0", author = "Szymon Wieloch <szymonwieloch.com>")]
struct Opts {
    /// Sets a custom config file. By default <bin dir>/teleport.yaml is opened.
    #[clap(short, long)]
    pub config: Option<String>,
}

#[derive(Deserialize)]
pub struct Config {
    pub addr: String,
}

pub fn parse_config() -> Config {
    let opts = Opts::parse();
    let cfg_path = match opts.config {
        Some(ref path) => PathBuf::from(path),
        None => {
            let mut path_buf = exe_dir().expect("Could not obtain executable directory");
            path_buf.push("teleport.yaml");
            path_buf
        }
    };
    let cfg_file = File::open(cfg_path).expect("Could not open configuration file");
    from_reader(cfg_file).expect("Could not parse configuration file")
}
