mod job;
mod jobs;
mod protocol;
mod server_cfg;
mod service;
mod utils;

use service::RemoteExecutorImp;
use std::fs;
use tonic::transport::{Identity, Server, ServerTlsConfig};
use tonic::Status;

use protocol::remote_executor_server::RemoteExecutorServer;

use server_cfg::parse_config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_config();
    let addr = config.addr.parse()?;

    // Build the auth interceptor, capturing the optional secret.
    let secret = config.secret.clone();
    let auth_interceptor = move |req: tonic::Request<()>| -> Result<tonic::Request<()>, Status> {
        if let Some(ref expected) = secret {
            let token = req
                .metadata()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(|s| s.to_string());

            match token {
                Some(t) if t == *expected => {}
                _ => {
                    return Err(Status::unauthenticated(
                        "Invalid or missing authorization token",
                    ));
                }
            }
        }
        Ok(req)
    };

    let mut builder = Server::builder();

    // Configure TLS if certificate and key are provided.
    if let (Some(ref cert_path), Some(ref key_path)) = (&config.tls_cert, &config.tls_key) {
        let cert = fs::read_to_string(cert_path)?;
        let key = fs::read_to_string(key_path)?;
        let identity = Identity::from_pem(cert, key);
        builder = builder.tls_config(ServerTlsConfig::new().identity(identity))?;
        println!("TLS enabled");
    }

    println!("Teleport server listening on {}", config.addr);

    builder
        .add_service(RemoteExecutorServer::with_interceptor(
            RemoteExecutorImp::new(config.limits),
            auth_interceptor,
        ))
        .serve(addr)
        .await?;

    Ok(())
}
