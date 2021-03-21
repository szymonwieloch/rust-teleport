mod client_cfg;
mod protocol;
mod utils;

use protocol::remote_executor_client::RemoteExecutorClient;
use protocol::Command;

use tonic::transport::Channel;

use client_cfg::{parse_config, Start, SubCommand};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (opts, cfg) = parse_config();

    let channel = Channel::from_shared(format!("http://{}", cfg.addr))?
        .connect()
        .await?;
    let client = RemoteExecutorClient::new(channel);

    // let client = RemoteExe
    match opts.subcmd {
        SubCommand::Start(start) => send_start(client, start).await?,
        SubCommand::Stop(_) => unimplemented!(),
        SubCommand::List => unimplemented!(),
        SubCommand::Status(_) => unimplemented!(),
        SubCommand::Log(_) => unimplemented!(),
    };
    Ok(())
}

async fn send_start(
    mut client: RemoteExecutorClient<tonic::transport::Channel>,
    start: Start,
) -> Result<(), tonic::Status> {
    let cmd = Command { command: start.cmd };
    let resp = client.start(cmd).await?;
    match &resp.get_ref().id {
        None => {
            return Err(tonic::Status::invalid_argument(
                "Response did not contain task ID",
            ))
        }
        Some(task_id) => println!("Started task with ID: {}", task_id.uuid),
    };
    Ok(())
}
