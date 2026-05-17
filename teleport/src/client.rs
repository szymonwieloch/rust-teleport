mod client_cfg;
mod protocol;
mod utils;

use protocol::job_status;
use protocol::remote_executor_client::RemoteExecutorClient;
use protocol::{Command, TaskId};
use tonic::transport::Channel;

use client_cfg::{Log, Start, Status, Stop, SubCommand, parse_config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (opts, cfg) = parse_config();

    let channel = Channel::from_shared(format!("http://{}", cfg.addr))?
        .connect()
        .await?;
    let client = RemoteExecutorClient::new(channel);

    match opts.subcmd {
        SubCommand::Start(start) => send_start(client, start).await?,
        SubCommand::Stop(stop) => send_stop(client, stop).await?,
        SubCommand::List => send_list(client).await?,
        SubCommand::Status(status) => send_status(client, status).await?,
        SubCommand::Log(log) => send_log(client, log).await?,
    };
    Ok(())
}

async fn send_start(
    mut client: RemoteExecutorClient<Channel>,
    start: Start,
) -> Result<(), tonic::Status> {
    let cmd = Command { command: start.cmd };
    let resp = client.start(cmd).await?;
    let job = resp.into_inner();
    match &job.id {
        None => Err(tonic::Status::invalid_argument(
            "Response did not contain task ID",
        )),
        Some(task_id) => {
            println!("Started task with ID: {}", task_id.uuid);
            Ok(())
        }
    }
}

async fn send_stop(
    mut client: RemoteExecutorClient<Channel>,
    stop: Stop,
) -> Result<(), tonic::Status> {
    let resp = client.stop(TaskId { uuid: stop.id }).await?;
    let job = resp.into_inner();
    match job.details {
        Some(job_status::Details::Stopped(s)) => {
            println!("Job stopped with exit code: {}", s.error_code);
        }
        _ => println!("Job stopped (status details unavailable)"),
    }
    Ok(())
}

async fn send_list(mut client: RemoteExecutorClient<Channel>) -> Result<(), tonic::Status> {
    let resp = client.list(()).await?;
    let list = resp.into_inner();

    if list.jobs.is_empty() {
        println!("No pending jobs.");
        return Ok(());
    }

    println!("{:<36}  {:8}  {:4}  COMMAND", "JOB ID", "STATUS", "LOGS");
    println!("{:-<36}  {:-<8}  {:-<4}  {:-<20}", "", "", "", "");

    for job in &list.jobs {
        let id = job
            .id
            .as_ref()
            .map(|t| t.uuid.as_str())
            .unwrap_or("unknown");
        let status_str = match &job.details {
            Some(job_status::Details::Pending(_)) => "RUNNING",
            Some(job_status::Details::Stopped(_)) => "STOPPED",
            None => "?",
        };
        let log_count = job.logs;
        let cmd_str = job
            .command
            .as_ref()
            .map(|c| c.command.join(" "))
            .unwrap_or_default();

        println!(
            "{:<36}  {:<8}  {:<4}  {}",
            id, status_str, log_count, cmd_str
        );
    }
    Ok(())
}

async fn send_status(
    mut client: RemoteExecutorClient<Channel>,
    status: Status,
) -> Result<(), tonic::Status> {
    let resp = client.get_status(TaskId { uuid: status.id }).await?;
    let job = resp.into_inner();

    let id = job
        .id
        .as_ref()
        .map(|t| t.uuid.as_str())
        .unwrap_or("unknown");
    let cmd = job
        .command
        .as_ref()
        .map(|c| c.command.join(" "))
        .unwrap_or_default();

    match job.details {
        Some(job_status::Details::Pending(p)) => {
            println!("Job:    {}", id);
            println!("Status: RUNNING");
            println!("Command: {}", cmd);
            println!("CPU:    {:.1}%", p.cpu_perc);
            println!("Memory: {:.1} MB", p.memory);
            println!("Logs:   {}", job.logs);
        }
        Some(job_status::Details::Stopped(s)) => {
            println!("Job:       {}", id);
            println!("Status:    STOPPED");
            println!("Command:   {}", cmd);
            println!("Exit code: {}", s.error_code);
        }
        None => {
            println!("Job:    {}", id);
            println!("Status: UNKNOWN");
        }
    }
    Ok(())
}

async fn send_log(
    mut client: RemoteExecutorClient<Channel>,
    log_args: Log,
) -> Result<(), tonic::Status> {
    let mut stream = client
        .logs(TaskId { uuid: log_args.id })
        .await?
        .into_inner();

    while let Some(log) = stream.message().await? {
        let prefix = match log.src() {
            protocol::LogSource::LsStdout => "\x1b[32mstdout\x1b[0m".to_string(),
            protocol::LogSource::LsStderr => "\x1b[31mstderr\x1b[0m".to_string(),
        };
        println!("[{}] {}", prefix, log.text);
    }

    println!("--- Log stream ended ---");
    Ok(())
}
