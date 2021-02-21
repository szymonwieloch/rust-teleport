#![allow(unused_variables)]

use super::protocol::remote_executor_server::RemoteExecutor;
use super::protocol::{Command, Log, StartedTask, Status, StoppedTask, TaskId};

use tokio_stream::wrappers::ReceiverStream;

pub struct RemoteExecutorImp {}

#[tonic::async_trait]
impl RemoteExecutor for RemoteExecutorImp {
    async fn start(
        &self,
        req: tonic::Request<Command>,
    ) -> Result<tonic::Response<StartedTask>, tonic::Status> {
        todo!()
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
