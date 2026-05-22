use clap::Parser;
use serde::Deserialize;
use serde_yaml::from_reader;
use teleport::utils::open_cfg_file;

#[derive(Parser)]
#[command(version = "1.0", author = "Szymon Wieloch <szymonwieloch.com>")]
struct Opts {
    /// Sets a custom config file. By default <bin dir>/teleport.yaml is opened.
    #[arg(short, long)]
    pub config: Option<String>,
}

/// Resource limits applied to spawned processes.
#[derive(Deserialize, Clone, Debug)]
pub struct ResourceLimits {
    /// Maximum CPU time in seconds (RLIMIT_CPU). Default: 60.
    #[serde(default = "default_cpu_limit")]
    pub cpu_seconds: u64,
    /// Maximum address space in bytes (RLIMIT_AS). Default: 100 MB.
    #[serde(default = "default_memory_limit")]
    pub memory_bytes: u64,
    /// Maximum file size in bytes (RLIMIT_FSIZE). Default: 10 MB.
    #[serde(default = "default_fsize_limit")]
    pub file_size_bytes: u64,
}

fn default_cpu_limit() -> u64 {
    60
}
fn default_memory_limit() -> u64 {
    100 * 1024 * 1024
}
fn default_fsize_limit() -> u64 {
    10 * 1024 * 1024
}

impl Default for ResourceLimits {
    fn default() -> Self {
        ResourceLimits {
            cpu_seconds: default_cpu_limit(),
            memory_bytes: default_memory_limit(),
            file_size_bytes: default_fsize_limit(),
        }
    }
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
    /// Custom resource limits. Only used when `limits` is true.
    #[serde(default)]
    pub resource_limits: ResourceLimits,
}

pub fn parse_config() -> Config {
    let opts = Opts::parse();
    let cfg_file = open_cfg_file(&opts.config, "teleport.yaml");
    from_reader(cfg_file).expect("Could not parse configuration file")
}
