use super::utils::open_cfg_file;
use clap::Parser;
use serde::Deserialize;
use serde_yaml::from_reader;

#[derive(Parser)]
#[command(version = "1.0", author = "Szymon Wieloch <szymonwieloch.com>")]
struct Opts {
    /// Sets a custom config file. By default <bin dir>/teleport.yaml is opened.
    #[arg(short, long)]
    pub config: Option<String>,
}

#[derive(Deserialize)]
pub struct Config {
    pub addr: String,
    /// Optional shared secret for bearer token authentication.
    #[serde(default)]
    pub secret: Option<String>,
    /// Path to TLS certificate PEM file (optional).
    #[serde(default)]
    pub tls_cert: Option<String>,
    /// Path to TLS private key PEM file (optional).
    #[serde(default)]
    pub tls_key: Option<String>,
    /// Enable resource limits on spawned processes.
    #[serde(default)]
    pub limits: bool,
}

pub fn parse_config() -> Config {
    let opts = Opts::parse();
    let cfg_file = open_cfg_file(&opts.config, "teleport.yaml");
    from_reader(cfg_file).expect("Could not parse configuration file")
}
