use super::utils::open_cfg_file;
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
    let cfg_file = open_cfg_file(opts.config, "teleport.yaml");
    from_reader(cfg_file).expect("Could not parse configuration file")
}
