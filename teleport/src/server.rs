mod protocol;
mod server_cfg;
mod service;
mod utils;

use service::RemoteExecutorImp;
use tonic::transport::Server;

use protocol::remote_executor_server::RemoteExecutorServer;

use server_cfg::parse_config;

#[allow(unused_variables)]
fn interceptor(req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
    // todo!()
    Ok(req)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_config();
    Server::builder()
        // .tls_config(
        //     ServerTlsConfig::new()
        //         .identity(identity)
        //         .client_ca_root(client_ca_cert),
        // )?
        .add_service(RemoteExecutorServer::with_interceptor(
            RemoteExecutorImp::new(),
            interceptor,
        ))
        .serve(config.addr.parse()?)
        .await?;

    Ok(())
}
