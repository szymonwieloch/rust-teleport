use super::utils::open_cfg_file;
use clap::Clap;
use serde::Deserialize;
use serde_yaml::from_reader;

#[derive(Clap)]
#[clap(version = "1.0", author = "Szymon Wieloch <szymonwieloch.com>")]
pub struct Opts {
    /// Sets a custom config file. By default <bin dir>/teleport.yaml is opened.
    #[clap(short, long)]
    pub config: Option<String>,
    #[clap(subcommand)]
    pub subcmd: SubCommand,
}

#[derive(Clap)]
pub enum SubCommand {
    Start(Start),
    Stop(Stop),
    Log(Log),
    ///Lists all remote tasks
    List,
    Status(Status),
}

/// Starts a remote task
#[derive(Clap)]
pub struct Start {
    /// Command to be ecexuted remotely
    pub cmd: Vec<String>,
}

/// Stops the remote task
#[derive(Clap)]
pub struct Stop {
    /// ID of the reomote task
    pub id: String,
}

///Prints to the screen output of the remote task
#[derive(Clap)]
pub struct Log {
    /// ID of the reomote task
    pub id: String,
}

///Prints status of the remote task
#[derive(Clap)]
pub struct Status {
    /// ID of the reomote task
    pub id: String,
}

#[derive(Deserialize)]
pub struct Config {
    pub addr: String,
}

pub fn parse_config() -> (Opts, Config) {
    let opts = Opts::parse();
    let cfg_file = open_cfg_file(&opts.config, "telecli.yaml");
    (
        opts,
        from_reader(cfg_file).expect("Could not parse configuration file"),
    )
}
