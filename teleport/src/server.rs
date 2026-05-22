mod server_cfg;

use server_cfg::parse_config;
use std::fs;
use teleport::protocol::remote_executor_server::RemoteExecutorServer;
use teleport::service::RemoteExecutorImp;
use tonic::Status;
use tonic::transport::{Identity, Server, ServerTlsConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing. Set RUST_LOG environment variable to control verbosity,
    // e.g. RUST_LOG=teleport=debug or RUST_LOG=info.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = parse_config();
    let addr = config.addr.parse()?;

    // Build the auth interceptor, capturing the optional secret.
    let secret = config.secret.clone();
    #[allow(clippy::result_large_err)]
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
                    return Err(Status::unauthenticated("Invalid or missing authorization token"));
                }
            }
        }
        Ok(req)
    };

    let mut builder = Server::builder();

    // Configure TLS if certificate and key are provided.
    if let (Some(cert_path), Some(key_path)) = (&config.tls_cert, &config.tls_key) {
        let cert = fs::read_to_string(cert_path)?;
        let key = fs::read_to_string(key_path)?;
        let identity = Identity::from_pem(cert, key);
        builder = builder.tls_config(ServerTlsConfig::new().identity(identity))?;
        tracing::info!("TLS enabled");
    }

    let service_impl = RemoteExecutorImp::new(if config.limits {
        Some(teleport::service::LimitsConfig {
            cpu_seconds: config.resource_limits.cpu_seconds,
            memory_bytes: config.resource_limits.memory_bytes,
            file_size_bytes: config.resource_limits.file_size_bytes,
        })
    } else {
        None
    });
    let router =
        builder.add_service(RemoteExecutorServer::with_interceptor(service_impl, auth_interceptor));

    tracing::info!("Teleport server listening on {}", config.addr);

    // Graceful shutdown: wait for Ctrl+C, then drain connections.
    let shutdown_signal = async {
        tokio::signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
        tracing::info!("Shutdown signal received, draining connections...");
    };

    router.serve_with_shutdown(addr, shutdown_signal).await?;

    tracing::info!("Server shut down gracefully");
    Ok(())
}
