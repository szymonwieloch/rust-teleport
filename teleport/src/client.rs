mod client_cfg;
mod protocol;
mod utils;

use client_cfg::parse_config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = parse_config();
    Ok(())
}
