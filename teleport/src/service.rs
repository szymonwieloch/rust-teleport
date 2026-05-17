#![allow(unused_variables)]

use super::job::Job;
use super::jobs::Jobs;
use super::protocol::remote_executor_server::RemoteExecutor;
use super::protocol::{
    job_status, Command, JobList, JobStatus, Log, PendingJobStatus, StoppedJobStatus, TaskId,
};
use prost_types::Timestamp;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

pub struct RemoteExecutorImp {
    jobs: Jobs,
    limits: bool,
}

impl RemoteExecutorImp {
    pub fn new(limits: bool) -> Self {
        RemoteExecutorImp {
            jobs: Jobs::new(),
            limits,
        }
    }
}

#[tonic::async_trait]
impl RemoteExecutor for RemoteExecutorImp {
    async fn start(
        &self,
        req: tonic::Request<Command>,
    ) -> Result<tonic::Response<JobStatus>, tonic::Status> {
        let args = &req.get_ref().command;
        if args.is_empty() {
            return Err(tonic::Status::invalid_argument(
                "Command needs to contain at least one word",
            ));
        }

        let job = Job::spawn(args.clone(), self.limits)
            .await
            .map_err(|e| tonic::Status::internal(format!("Failed to spawn process: {}", e)))?;

        let proc_uuid = Uuid::new_v4();
        let pending = job.pending_status().await.unwrap_or(PendingJobStatus {
            cpu_perc: 0.0,
            memory: 0.0,
        });

        self.jobs.insert(proc_uuid, Arc::clone(&job)).await;

        let job_status = JobStatus {
            id: Some(TaskId {
                uuid: proc_uuid.to_string(),
            }),
            started: Some(system_time_to_proto(job.started)),
            logs: job.log_count().await,
            command: Some(Command {
                command: args.clone(),
            }),
            details: Some(job_status::Details::Pending(pending)),
        };
        Ok(tonic::Response::new(job_status))
    }

    async fn stop(
        &self,
        req: tonic::Request<TaskId>,
    ) -> Result<tonic::Response<JobStatus>, tonic::Status> {
        let task_id = req.into_inner();
        let uuid = parse_uuid(&task_id)?;

        let final_status = self
            .jobs
            .stop(&uuid)
            .await
            .ok_or_else(|| tonic::Status::not_found("Job not found"))?;

        let status = match final_status {
            super::job::JobStatusEnum::Stopped {
                exit_code,
                stopped_at,
            } => JobStatus {
                id: Some(TaskId {
                    uuid: uuid.to_string(),
                }),
                details: Some(job_status::Details::Stopped(StoppedJobStatus {
                    error_code: exit_code,
                    stopped: Some(system_time_to_proto(stopped_at)),
                })),
                ..Default::default()
            },
            _ => {
                return Err(tonic::Status::internal(
                    "Job stop returned unexpected running status",
                ))
            }
        };

        Ok(tonic::Response::new(status))
    }

    async fn logs(
        &self,
        req: tonic::Request<TaskId>,
    ) -> Result<tonic::Response<Self::LogsStream>, tonic::Status> {
        let task_id = req.into_inner();
        let uuid = parse_uuid(&task_id)?;

        let job = self
            .jobs
            .find(&uuid)
            .await
            .ok_or_else(|| tonic::Status::not_found("Job not found"))?;

        let (tx, rx) = mpsc::channel(16);

        tokio::spawn(async move {
            let mut index: usize = 0;
            loop {
                let entries = job.get_logs(index).await;
                if entries.is_empty() {
                    // Job finished with no more logs
                    break;
                }
                for entry in &entries {
                    let log = Job::entry_to_proto(entry);
                    if tx.send(Ok(log)).await.is_err() {
                        // Client disconnected
                        return;
                    }
                }
                index += entries.len();
            }
        });

        Ok(tonic::Response::new(ReceiverStream::new(rx)))
    }

    async fn get_status(
        &self,
        req: tonic::Request<TaskId>,
    ) -> Result<tonic::Response<JobStatus>, tonic::Status> {
        let task_id = req.into_inner();
        let uuid = parse_uuid(&task_id)?;

        let job = self
            .jobs
            .find(&uuid)
            .await
            .ok_or_else(|| tonic::Status::not_found("Job not found"))?;

        let status = job.status().await;
        let logs = job.log_count().await;

        let details = match status {
            super::job::JobStatusEnum::Running => {
                let pending = job.pending_status().await.unwrap_or(PendingJobStatus {
                    cpu_perc: 0.0,
                    memory: 0.0,
                });
                job_status::Details::Pending(pending)
            }
            super::job::JobStatusEnum::Stopped {
                exit_code,
                stopped_at,
            } => job_status::Details::Stopped(StoppedJobStatus {
                error_code: exit_code,
                stopped: Some(system_time_to_proto(stopped_at)),
            }),
        };

        Ok(tonic::Response::new(JobStatus {
            id: Some(TaskId {
                uuid: uuid.to_string(),
            }),
            started: Some(system_time_to_proto(job.started)),
            logs,
            command: Some(Command {
                command: job.command.clone(),
            }),
            details: Some(details),
        }))
    }

    async fn list(
        &self,
        _req: tonic::Request<()>,
    ) -> Result<tonic::Response<JobList>, tonic::Status> {
        let jobs = self.jobs.list().await;

        let mut job_statuses = Vec::with_capacity(jobs.len());
        for (uuid, job) in &jobs {
            let status = job.status().await;
            let logs = job.log_count().await;

            let details = match status {
                super::job::JobStatusEnum::Running => {
                    let pending = job.pending_status().await.unwrap_or(PendingJobStatus {
                        cpu_perc: 0.0,
                        memory: 0.0,
                    });
                    job_status::Details::Pending(pending)
                }
                super::job::JobStatusEnum::Stopped {
                    exit_code,
                    stopped_at,
                } => job_status::Details::Stopped(StoppedJobStatus {
                    error_code: exit_code,
                    stopped: Some(system_time_to_proto(stopped_at)),
                }),
            };

            job_statuses.push(JobStatus {
                id: Some(TaskId {
                    uuid: uuid.to_string(),
                }),
                started: Some(system_time_to_proto(job.started)),
                logs,
                command: Some(Command {
                    command: job.command.clone(),
                }),
                details: Some(details),
            });
        }

        Ok(tonic::Response::new(JobList { jobs: job_statuses }))
    }

    type LogsStream = ReceiverStream<Result<Log, tonic::Status>>;
}

/// Convert a UUID string from a TaskId into a Uuid, or return InvalidArgument.
#[allow(clippy::result_large_err)]
fn parse_uuid(task_id: &TaskId) -> Result<Uuid, tonic::Status> {
    Uuid::parse_str(&task_id.uuid)
        .map_err(|_| tonic::Status::invalid_argument("Invalid UUID format"))
}

/// Convert std::time::Instant (wall-clock based) or SystemTime into protobuf Timestamp.
/// We use SystemTime::now() offset from Instant::now().
fn system_time_to_proto(instant: std::time::Instant) -> Timestamp {
    let elapsed = instant.elapsed();
    let now = SystemTime::now();
    let then = now.checked_sub(elapsed).unwrap_or(SystemTime::UNIX_EPOCH);
    let duration = then
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    Timestamp {
        seconds: duration.as_secs() as i64,
        nanos: duration.subsec_nanos() as i32,
    }
}
