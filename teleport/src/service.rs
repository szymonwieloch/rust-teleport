#![allow(unused_variables)]

use super::protocol::remote_executor_server::RemoteExecutor;
use super::protocol::{Command, Log, StartedTask, Status, StoppedTask, TaskId};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Mutex;
use tokio::process::{Child, Command as AsyncCommand};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

pub struct RemoteExecutorImp {
    children: Mutex<HashMap<Uuid, Child>>,
}

impl RemoteExecutorImp {
    pub fn new() -> Self {
        RemoteExecutorImp {
            children: Mutex::new(HashMap::new()),
        }
    }
}

#[tonic::async_trait]
impl RemoteExecutor for RemoteExecutorImp {
    async fn start(
        &self,
        req: tonic::Request<Command>,
    ) -> Result<tonic::Response<StartedTask>, tonic::Status> {
        let args = &req.get_ref().command;
        if args.len() < 1 {
            return Err(tonic::Status::invalid_argument(
                "Command need to contain at least one word",
            ));
        }
        let mut cmd = AsyncCommand::new(&args[0])
            .args(&args[1..])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            // unsafe { cmd.pre_exec(|| limit_resources()) };
            .spawn()?;
        let stdout = cmd
            .stdout
            .take()
            .ok_or_else(|| tonic::Status::internal("Could not start the process"))?;
        let stderr = cmd
            .stderr
            .take()
            .ok_or_else(|| tonic::Status::internal("Could not start the process"))?;
        let proc_uuid = Uuid::new_v4();
        let mut children = self.children.lock().expect("Posoned mutex");
        children.insert(proc_uuid, cmd);
        let started_task = StartedTask {
            id: Some(TaskId {
                uuid: proc_uuid.to_string(),
            }),
        };
        Ok(tonic::Response::new(started_task))
    }
    async fn stop(
        &self,
        req: tonic::Request<TaskId>,
    ) -> Result<tonic::Response<StoppedTask>, tonic::Status> {
        todo!()
    }
    async fn logs(
        &self,
        req: tonic::Request<TaskId>,
    ) -> Result<tonic::Response<Self::LogsStream>, tonic::Status> {
        todo!()
    }
    async fn get_status(
        &self,
        req: tonic::Request<TaskId>,
    ) -> Result<tonic::Response<Status>, tonic::Status> {
        todo!()
    }
    type LogsStream = ReceiverStream<Result<Log, tonic::Status>>;
}
