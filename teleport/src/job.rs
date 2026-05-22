use crate::protocol::{Log, LogSource, PendingJobStatus};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Instant, SystemTime};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command as AsyncCommand};
use tokio::sync::{Mutex, Notify, RwLock};

/// A single log line captured from the process stdout or stderr.
#[derive(Clone, Debug)]
pub struct LogEntry {
    pub text: String,
    pub source: LogSource,
    pub timestamp: SystemTime,
}

/// Whether the job is still running or has terminated.
#[derive(Clone, Debug, PartialEq)]
pub enum JobStatusEnum {
    Running,
    Stopped { exit_code: i32, stopped_at: Instant },
}

/// Represents a remotely executed process and all its tracked state.
pub struct Job {
    /// The original command that was executed.
    pub command: Vec<String>,
    /// When the job was started (wall clock).
    pub started: Instant,
    /// The tokio process handle. Wrapped in Option so `stop()` can take ownership.
    child: Mutex<Option<Child>>,
    /// The OS process ID, stored separately so we can kill the process even
    /// after the Child handle has been taken by the background wait task.
    pid: Option<u32>,
    /// Current job status.
    status: RwLock<JobStatusEnum>,
    /// Cached log entries from both stdout and stderr.
    logs: Mutex<Vec<LogEntry>>,
    /// Notifies blocked log readers when new log lines arrive.
    notify: Notify,
}

impl Job {
    /// Spawn a new job from the given command.
    /// If `limits` is true, resource limits (CPU, memory, file size) are applied
    /// via setrlimit before executing the child process.
    /// Launches background tasks that read stdout and stderr line-by-line
    /// into the log buffer.
    pub async fn spawn(command: Vec<String>, limits: bool) -> Result<Arc<Self>, std::io::Error> {
        let mut cmd = AsyncCommand::new(&command[0]);
        if command.len() > 1 {
            cmd.args(&command[1..]);
        }

        if limits {
            unsafe {
                cmd.pre_exec(|| {
                    limit_resources();
                    Ok(())
                });
            }
        }

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdout = child.stdout.take().expect("stdout not piped");
        let stderr = child.stderr.take().expect("stderr not piped");
        let pid = child.id();

        let job = Arc::new(Job {
            command,
            started: Instant::now(),
            child: Mutex::new(Some(child)),
            pid,
            status: RwLock::new(JobStatusEnum::Running),
            logs: Mutex::new(Vec::new()),
            notify: Notify::new(),
        });

        // Spawn background reader for stdout
        let job_stdout = Arc::clone(&job);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                job_stdout.push_log(line, LogSource::LsStdout).await;
            }
        });

        // Spawn background reader for stderr
        let job_stderr = Arc::clone(&job);
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                job_stderr.push_log(line, LogSource::LsStderr).await;
            }
        });

        // Spawn background task to wait for process exit and update status.
        let job_wait = Arc::clone(&job);
        tokio::spawn(async move {
            // Take the child out of the mutex so we don't hold the lock
            // while waiting for process exit (which could block stop()).
            let mut child = job_wait.child.lock().await.take();
            let exit_code = match child {
                Some(ref mut c) => match c.wait().await {
                    Ok(status) => status.code().unwrap_or(-1),
                    Err(_) => -1,
                },
                None => return, // Already stopped via explicit stop()
            };

            let stopped_at = Instant::now();
            *job_wait.status.write().await = JobStatusEnum::Stopped {
                exit_code,
                stopped_at,
            };

            // Wake up any blocked log readers
            job_wait.notify.notify_waiters();
        });

        Ok(job)
    }

    /// Append a log line to the buffer and notify any blocked readers.
    async fn push_log(&self, text: String, source: LogSource) {
        let entry = LogEntry {
            text,
            source,
            timestamp: SystemTime::now(),
        };
        {
            let mut logs = self.logs.lock().await;
            logs.push(entry);
        }
        self.notify.notify_waiters();
    }

    /// Return a snapshot of the current job status.
    pub async fn status(&self) -> JobStatusEnum {
        self.status.read().await.clone()
    }

    /// Kill the process and record exit information.
    /// Returns the final status.
    pub async fn stop(&self) -> JobStatusEnum {
        let mut child_opt = self.child.lock().await;
        if let Some(mut child) = child_opt.take() {
            // Try graceful kill first, then force
            let _ = child.start_kill();
            let exit_status = child.wait().await;
            let exit_code = exit_status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);
            let stopped_at = Instant::now();

            let final_status = JobStatusEnum::Stopped {
                exit_code,
                stopped_at,
            };
            *self.status.write().await = final_status.clone();

            // Wake up any log readers — the process is done
            self.notify.notify_waiters();

            final_status
        } else {
            // Child was already taken (by background wait task or previous stop).
            // Kill the process by PID if we have one, then wait for the status.
            if let Some(pid) = self.pid {
                // Send SIGKILL to the process — the background wait task will
                // pick up the exit and update the status.
                unsafe { libc::kill(pid as i32, libc::SIGKILL); }
            }

            // Wait for the status to transition to Stopped.
            loop {
                let status = self.status.read().await.clone();
                if let JobStatusEnum::Stopped { .. } = status {
                    return status;
                }
                drop(status);
                // Wait for the background task to update the status
                self.notify.notified().await;
            }
        }
    }

    /// Return cached log entries starting from `after_index`.
    /// If no new entries are available and the job is still running,
    /// blocks until new data arrives.
    pub async fn get_logs(&self, after_index: usize) -> Vec<LogEntry> {
        loop {
            let logs = self.logs.lock().await;
            if after_index < logs.len() {
                return logs[after_index..].to_vec();
            }
            // Release the lock before awaiting on notify
            drop(logs);

            // If the job has stopped, return whatever we have (empty)
            if *self.status.read().await != JobStatusEnum::Running {
                return Vec::new();
            }

            // Wait for new log data
            self.notify.notified().await;
        }
    }

    /// Total number of log entries so far.
    pub async fn log_count(&self) -> u32 {
        self.logs.lock().await.len() as u32
    }

    /// Build a protobuf `Log` from a `LogEntry`.
    pub fn entry_to_proto(entry: &LogEntry) -> Log {
        Log {
            text: entry.text.clone(),
            src: entry.source as i32,
            timestamp: Some(prost_types::Timestamp {
                seconds: entry
                    .timestamp
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
                nanos: 0,
            }),
        }
    }

    /// Compute pending (running) status with CPU percentage and memory usage.
    /// Returns `None` if the process has already stopped.
    pub async fn pending_status(&self) -> Option<PendingJobStatus> {
        let child_lock = self.child.lock().await;
        let child = match child_lock.as_ref() {
            Some(c) => c,
            None => return None,
        };

        let pid = match child.id() {
            Some(id) => id,
            None => return None,
        };

        // Read CPU usage from /proc/<pid>/stat
        let cpu_perc = read_cpu_perc(pid).await.unwrap_or(0.0);

        // Read memory usage from /proc/<pid>/status (VmRSS)
        let memory_mb = read_memory_mb(pid).await.unwrap_or(0.0);

        Some(PendingJobStatus {
            cpu_perc,
            memory: memory_mb,
        })
    }
}

/// Read CPU usage percentage for a process from /proc/<pid>/stat.
/// Returns a value between 0.0 and 100.0 * number_of_cores.
async fn read_cpu_perc(pid: u32) -> Result<f32, std::io::Error> {
    let stat = tokio::fs::read_to_string(format!("/proc/{}/stat", pid)).await?;

    // /proc/[pid]/stat fields are space-separated; the process name (field 2)
    // may contain spaces and is wrapped in parentheses. We find the closing ')'
    // and parse the remaining fields.
    let close_paren = stat.rfind(')').unwrap_or(0);
    let after_comm = &stat[close_paren + 2..]; // skip ") "
    let fields: Vec<&str> = after_comm.split_whitespace().collect();

    // Field indices after comm (0-based in after_comm):
    // 11 = starttime, 13 = utime, 14 = stime, 15 = cutime, 16 = cstime
    if fields.len() < 15 {
        return Ok(0.0);
    }

    let utime: u64 = fields[11].parse().unwrap_or(0);
    let stime: u64 = fields[12].parse().unwrap_or(0);

    // Read system uptime from /proc/uptime
    let uptime_str = tokio::fs::read_to_string("/proc/uptime").await?;
    let uptime: f64 = uptime_str
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);

    // Read CLK_TCK (ticks per second)
    let clk_tck = 100.0; // Linux default; could read from sysconf(_SC_CLK_TCK)

    let total_time = (utime + stime) as f64 / clk_tck;
    if uptime > 0.0 {
        // Simple approximation: CPU% = total_time / uptime * 100
        let cpu = (total_time / uptime * 100.0) as f32;
        Ok(cpu.min(100.0))
    } else {
        Ok(0.0)
    }
}

/// Read memory usage in MB for a process from /proc/<pid>/status.
async fn read_memory_mb(pid: u32) -> Result<f32, std::io::Error> {
    let status = tokio::fs::read_to_string(format!("/proc/{}/status", pid)).await?;
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let kb: u64 = line
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            return Ok(kb as f32 / 1024.0); // KB → MB
        }
    }
    Ok(0.0)
}

/// Apply resource limits to the current (child) process via setrlimit.
/// This function must be called from `pre_exec` — it runs in the forked child
/// before the new program is executed.
unsafe fn limit_resources() {
    // CPU time limit: 60 seconds of total CPU time.
    let cpu_limit = libc::rlimit {
        rlim_cur: 60,
        rlim_max: 60,
    };
    unsafe { libc::setrlimit(libc::RLIMIT_CPU, &cpu_limit) };

    // Address space (memory) limit: 100 MB.
    let mem_limit = libc::rlimit {
        rlim_cur: 100 * 1024 * 1024,
        rlim_max: 100 * 1024 * 1024,
    };
    unsafe { libc::setrlimit(libc::RLIMIT_AS, &mem_limit) };

    // File size limit: 10 MB per file.
    let fsize_limit = libc::rlimit {
        rlim_cur: 10 * 1024 * 1024,
        rlim_max: 10 * 1024 * 1024,
    };
    unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &fsize_limit) };
}
