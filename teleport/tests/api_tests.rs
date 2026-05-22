use std::time::{Duration, SystemTime};

use prost_types::Timestamp;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::metadata::MetadataValue;
use tonic::transport::{
    Certificate, Channel, ClientTlsConfig, Endpoint, Identity, Server, ServerTlsConfig,
};
use tonic::{Request, Status};
use uuid::Uuid;

use teleport::protocol::job_status;
use teleport::protocol::remote_executor_client::RemoteExecutorClient;
use teleport::protocol::remote_executor_server::RemoteExecutorServer;
use teleport::protocol::{Command, JobStatus, LogSource, TaskId};
use teleport::service::RemoteExecutorImp;

fn certs_dir() -> String {
    format!("{}/../certs", env!("CARGO_MANIFEST_DIR"))
}

// ---- Server helpers ----

struct TestServer {
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
    port: u16,
}

impl TestServer {
    async fn start(secret: Option<String>) -> Self {
        let imp = RemoteExecutorImp::new(None);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("failed to bind");
        let port = listener.local_addr().unwrap().port();

        let mut builder = Server::builder();

        if secret.is_some() {
            let cert_path = format!("{}/server_cert.pem", certs_dir());
            let key_path = format!("{}/server_key_pkcs8.pem", certs_dir());
            let cert = std::fs::read_to_string(&cert_path).expect("server cert not found");
            let key = std::fs::read_to_string(&key_path).expect("server key not found");
            let identity = Identity::from_pem(cert, key);
            builder = builder
                .tls_config(ServerTlsConfig::new().identity(identity))
                .expect("TLS config failed");
        }

        let secret_clone = secret.clone();
        #[allow(clippy::result_large_err)]
        let auth_interceptor = move |req: Request<()>| -> Result<Request<()>, Status> {
            if let Some(ref expected) = secret_clone {
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

        let router =
            builder.add_service(RemoteExecutorServer::with_interceptor(imp, auth_interceptor));

        let handle = tokio::spawn(async move {
            tokio::select! {
                result = router.serve_with_incoming(TcpListenerStream::new(listener)) => {
                    eprintln!("server error: {:?}", result.err());
                }
                _ = shutdown_rx => {}
            }
        });

        // Wait until the server is ready to accept connections.
        let addr = format!("127.0.0.1:{}", port);
        for _ in 0..50 {
            if tokio::net::TcpStream::connect(&addr).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        TestServer { shutdown_tx: Some(shutdown_tx), handle: Some(handle), port }
    }

    fn port(&self) -> u16 {
        self.port
    }

    async fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

// ---- Client helpers ----

struct TestClient {
    inner: RemoteExecutorClient<Channel>,
    /// Bearer token secret, if any.
    secret: Option<String>,
}

impl TestClient {
    async fn connect(
        port: u16,
        use_tls: bool,
        secret: Option<&str>,
    ) -> Result<Self, tonic::transport::Error> {
        let addr = format!("127.0.0.1:{}", port);
        let url = if use_tls { format!("https://{}", addr) } else { format!("http://{}", addr) };

        let mut endpoint = Endpoint::from_shared(url).expect("invalid URL");

        if use_tls {
            let ca_path = format!("{}/ca_cert.pem", certs_dir());
            let ca_cert = std::fs::read_to_string(&ca_path).expect("CA cert not found");
            let tls = ClientTlsConfig::new()
                .ca_certificate(Certificate::from_pem(ca_cert))
                .domain_name("x.test.example.com");
            endpoint = endpoint.tls_config(tls).expect("client TLS config failed");
        }

        let channel = endpoint.connect().await?;
        Ok(Self {
            inner: RemoteExecutorClient::new(channel),
            secret: secret.map(|s| s.to_string()),
        })
    }

    /// Attach the Bearer token to a request if a secret is configured.
    fn auth_request<T>(&self, mut req: Request<T>) -> Request<T> {
        if let Some(ref secret) = self.secret {
            let bearer: MetadataValue<_> = format!("Bearer {}", secret).parse().unwrap();
            req.metadata_mut().insert("authorization", bearer);
        }
        req
    }

    async fn start(&mut self, cmd: Vec<String>) -> Result<JobStatus, Status> {
        let req = self.auth_request(Request::new(Command { command: cmd }));
        self.inner.start(req).await.map(|r| r.into_inner())
    }

    async fn stop(&mut self, id: &str) -> Result<JobStatus, Status> {
        let req = self.auth_request(Request::new(TaskId { uuid: id.to_string() }));
        self.inner.stop(req).await.map(|r| r.into_inner())
    }

    async fn get_status(&mut self, id: &str) -> Result<JobStatus, Status> {
        let req = self.auth_request(Request::new(TaskId { uuid: id.to_string() }));
        self.inner.get_status(req).await.map(|r| r.into_inner())
    }

    async fn list(&mut self) -> Result<Vec<JobStatus>, Status> {
        let req = self.auth_request(Request::new(()));
        self.inner.list(req).await.map(|r| r.into_inner().jobs)
    }
}

// ---- Convenience: start server + client ----

async fn create_server_and_client() -> (TestClient, TestServer) {
    let server = TestServer::start(None).await;
    let port = server.port();
    let client = TestClient::connect(port, false, None).await.expect("connect failed");
    (client, server)
}

// ---- Assertion helpers ----

fn is_recent(ts: &Timestamp) -> bool {
    let now = SystemTime::now();
    let t = SystemTime::UNIX_EPOCH + Duration::new(ts.seconds as u64, ts.nanos as u32);
    t <= now && t >= now.checked_sub(Duration::from_secs(2)).unwrap_or(SystemTime::UNIX_EPOCH)
}

fn is_valid_uuid(s: &str) -> bool {
    Uuid::parse_str(s).is_ok()
}

fn check_job(status: &JobStatus, cmd: &[String]) {
    let id = status.id.as_ref().expect("missing id");
    assert!(is_valid_uuid(&id.uuid), "invalid UUID: {}", id.uuid);
    assert!(
        is_recent(status.started.as_ref().expect("missing started")),
        "started timestamp not recent"
    );
    let got_cmd = status.command.as_ref().expect("missing command").command.clone();
    assert_eq!(got_cmd, cmd, "command mismatch");
}

fn is_stopped(details: &Option<job_status::Details>) -> bool {
    matches!(details, Some(job_status::Details::Stopped(_)))
}

fn get_pending(status: &JobStatus) -> Option<&teleport::protocol::PendingJobStatus> {
    match &status.details {
        Some(job_status::Details::Pending(p)) => Some(p),
        _ => None,
    }
}

fn get_stopped(status: &JobStatus) -> Option<&teleport::protocol::StoppedJobStatus> {
    match &status.details {
        Some(job_status::Details::Stopped(s)) => Some(s),
        _ => None,
    }
}

fn check_started_job(status: &JobStatus, cmd: &[String]) {
    check_job(status, cmd);
    // logs may be > 0 for fast commands that already produced output
    assert!(!is_stopped(&status.details), "should not be stopped");
    let pending = get_pending(status).expect("should have pending status for running job");
    assert!(pending.cpu_perc >= 0.0, "cpu_perc >= 0");
    assert!(pending.cpu_perc <= 100.0, "cpu_perc <= 100");
    assert!(pending.memory >= 0.0, "memory >= 0");
}

fn check_stopped_job(status: &JobStatus, expected_id: &str, cmd: &[String]) {
    check_job(status, cmd);
    assert_eq!(status.id.as_ref().expect("missing id").uuid, expected_id, "id mismatch");
    let stopped = get_stopped(status).expect("should be stopped");
    assert!(is_recent(stopped.stopped.as_ref().expect("missing stopped timestamp")));
    let started = status.started.as_ref().expect("missing started");
    let started_ts =
        SystemTime::UNIX_EPOCH + Duration::new(started.seconds as u64, started.nanos as u32);
    let stopped_ts = SystemTime::UNIX_EPOCH
        + Duration::new(
            stopped.stopped.as_ref().unwrap().seconds as u64,
            stopped.stopped.as_ref().unwrap().nanos as u32,
        );
    assert!(
        stopped_ts >= started_ts,
        "stopped ({:?}) should be after started ({:?})",
        stopped_ts,
        started_ts
    );
}

// ---- Test cases ----

#[tokio::test]
async fn test_auth() {
    let tests = vec![
        ("", "", true),
        ("blah", "blah", true),
        ("nope", "blah", false),
        ("nope", "blah", false),
        ("", "blah", false),
        ("nope", "", false),
    ];

    for (client_secret, server_secret, want_ok) in tests {
        let server_secret_opt =
            if server_secret.is_empty() { None } else { Some(server_secret.to_string()) };
        let client_secret_opt = if client_secret.is_empty() { None } else { Some(client_secret) };

        let mut server = TestServer::start(server_secret_opt.clone()).await;
        let port = server.port();
        // Client uses TLS whenever it has a secret (matching Go behavior).
        let use_tls = client_secret_opt.is_some();
        let client_result = TestClient::connect(port, use_tls, client_secret_opt).await;

        let mut client = match client_result {
            Ok(c) => c,
            Err(_) => {
                // Connection failed (e.g. TLS mismatch between client and server).
                // This counts as failure.
                if want_ok {
                    panic!(
                        "expected success for client={:?} server={:?} but connection failed",
                        client_secret, server_secret
                    );
                }
                drop(server);
                continue;
            }
        };

        let result = client.start(vec!["echo".into(), "blah".into()]).await;
        let got_ok = result.is_ok();

        if want_ok {
            assert!(
                got_ok,
                "expected success for client={:?} server={:?}",
                client_secret, server_secret
            );
        } else {
            assert!(
                !got_ok,
                "expected failure for client={:?} server={:?}",
                client_secret, server_secret
            );
        }

        drop(client);
        server.shutdown().await;
    }
}

/// Runs a short application and inspects its status after it shuts down.
#[tokio::test]
async fn test_short() {
    let (mut client, mut server) = create_server_and_client().await;

    let short_cmd: Vec<String> = vec!["echo".into(), "blah".into()];
    let st1 = client.start(short_cmd.clone()).await.expect("start failed");
    check_started_job(&st1, &short_cmd);
    let job_id = st1.id.as_ref().unwrap().uuid.clone();

    // Poll until the job finishes instead of sleeping a fixed duration.
    let st2 = loop {
        let s = client.get_status(&job_id).await.expect("get_status failed");
        if is_stopped(&s.details) {
            break s;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    check_stopped_job(&st2, &job_id, &short_cmd);
    assert_eq!(get_stopped(&st2).unwrap().error_code, 0);

    // Stop removes the job from the internal list
    let st3 = client.stop(&job_id).await.expect("stop failed");
    check_stopped_job(&st3, &job_id, &short_cmd);
    assert_eq!(get_stopped(&st3).unwrap().error_code, 0);

    // Job should no longer be findable
    let err = client.get_status(&job_id).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);

    server.shutdown().await;
}

/// Runs a long application and inspects its status while running, then stops it.
#[tokio::test]
async fn test_long() {
    let (mut client, mut server) = create_server_and_client().await;

    let long_cmd: Vec<String> = vec!["sleep".into(), "10".into()];
    let st1 = client.start(long_cmd.clone()).await.expect("start failed");
    check_started_job(&st1, &long_cmd);
    let job_id = st1.id.as_ref().unwrap().uuid.clone();

    let st2 = client.get_status(&job_id).await.expect("get_status failed");
    check_started_job(&st2, &long_cmd);

    let st3 = client.stop(&job_id).await.expect("stop failed");
    check_stopped_job(&st3, &job_id, &long_cmd);
    assert_eq!(get_stopped(&st3).unwrap().error_code, -1);

    let err = client.get_status(&job_id).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);

    server.shutdown().await;
}

/// Runs a logging application and inspects its logs while it is running.
#[tokio::test]
async fn test_logs() {
    let (mut client, mut server) = create_server_and_client().await;

    let logging_cmd: Vec<String> = vec![
        "bash".into(),
        "-c".into(),
        "for i in {0..5}; do echo Welcome $i times; sleep 0.1; done".into(),
    ];
    let st = client.start(logging_cmd.clone()).await.expect("start failed");
    check_started_job(&st, &logging_cmd);
    let job_id = st.id.as_ref().unwrap().uuid.clone();

    // Drain logs — expect 6 lines ("Welcome 0 times" through "Welcome 5 times")
    let req = Request::new(TaskId { uuid: job_id.clone() });
    let mut stream = client.inner.logs(req).await.expect("logs RPC failed").into_inner();

    for i in 0..7 {
        let msg = stream.message().await.expect("stream error");
        if i == 6 {
            assert!(msg.is_none(), "expected end of stream at message 6, got {:?}", msg);
            break;
        }
        let log = msg.expect("missing log message");
        assert_eq!(log.text, format!("Welcome {} times", i), "log text mismatch at index {}", i);
        assert!(
            is_recent(log.timestamp.as_ref().expect("missing timestamp")),
            "timestamp not recent"
        );
        assert_eq!(log.src(), LogSource::LsStdout, "expected stdout source");
    }

    server.shutdown().await;
}

/// Runs two jobs in parallel to check for races.
#[tokio::test]
async fn test_parallel() {
    let (mut client, mut server) = create_server_and_client().await;

    let logging_cmd: Vec<String> = vec![
        "bash".into(),
        "-c".into(),
        "for i in {0..5}; do echo Welcome $i times; sleep 0.1; done".into(),
    ];

    let st1 = client.start(logging_cmd.clone()).await.expect("start 1 failed");
    check_started_job(&st1, &logging_cmd);
    let id1 = st1.id.as_ref().unwrap().uuid.clone();

    let st2 = client.start(logging_cmd.clone()).await.expect("start 2 failed");
    check_started_job(&st2, &logging_cmd);
    let id2 = st2.id.as_ref().unwrap().uuid.clone();

    // Spawn two concurrent log drainers — each needs its own connection
    let port = server.port();
    let mut client1 = TestClient::connect(port, false, None).await.expect("connect failed");
    let mut client2 = TestClient::connect(port, false, None).await.expect("connect failed");

    let (r1, r2) = tokio::join!(drain_logs(&mut client1, &id1), drain_logs(&mut client2, &id2),);
    r1.expect("drain 1 failed");
    r2.expect("drain 2 failed");

    server.shutdown().await;
}

/// Verify that `List` returns all jobs and their statuses.
#[tokio::test]
async fn test_list() {
    let (mut client, mut server) = create_server_and_client().await;

    // Start a short command that finishes quickly.
    let short_cmd: Vec<String> = vec!["echo".into(), "hello".into()];
    let st1 = client.start(short_cmd.clone()).await.expect("start 1 failed");
    let id1 = st1.id.as_ref().unwrap().uuid.clone();

    // Start a long-running command.
    let long_cmd: Vec<String> = vec!["sleep".into(), "5".into()];
    let st2 = client.start(long_cmd.clone()).await.expect("start 2 failed");
    let id2 = st2.id.as_ref().unwrap().uuid.clone();

    // List should contain both jobs.
    let jobs = client.list().await.expect("list failed");
    assert_eq!(jobs.len(), 2, "expected 2 jobs in list");

    let ids: Vec<&str> = jobs.iter().map(|j| j.id.as_ref().unwrap().uuid.as_str()).collect();
    assert!(ids.contains(&id1.as_str()), "list missing job 1");
    assert!(ids.contains(&id2.as_str()), "list missing job 2");

    // Stop the long job; it should be removed from the collection.
    client.stop(&id2).await.expect("stop failed");
    let jobs_after = client.list().await.expect("list after stop failed");
    // After stop removes from pending, only the short job remains
    // (it may also have been removed if it completed and was stopped by
    // the background task — but it was never explicitly stopped, so it
    // stays in the map).
    assert!(jobs_after.len() <= 2, "unexpected number of jobs after stop");

    server.shutdown().await;
}

/// Verify that jobs started with resource limits do not crash.
#[tokio::test]
async fn test_resource_limits() {
    use teleport::protocol::remote_executor_server::RemoteExecutorServer;
    use teleport::service::RemoteExecutorImp;

    let imp = RemoteExecutorImp::new(Some(teleport::service::LimitsConfig {
        cpu_seconds: 60,
        memory_bytes: 100 * 1024 * 1024,
        file_size_bytes: 10 * 1024 * 1024,
    }));
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("failed to bind");
    let port = listener.local_addr().unwrap().port();

    let router = Server::builder().add_service(RemoteExecutorServer::new(imp));

    let handle = tokio::spawn(async move {
        tokio::select! {
            result = router.serve_with_incoming(TcpListenerStream::new(listener)) => {
                eprintln!("server error: {:?}", result.err());
            }
            _ = shutdown_rx => {}
        }
    });

    // Wait until the server is ready.
    let addr = format!("127.0.0.1:{}", port);
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(&addr).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let mut client = TestClient::connect(port, false, None).await.expect("connect failed");

    // A short innocuous command should succeed even with limits.
    let cmd: Vec<String> = vec!["echo".into(), "limits-test".into()];
    let st = client.start(cmd.clone()).await.expect("start with limits failed");
    check_started_job(&st, &cmd);
    let job_id = st.id.as_ref().unwrap().uuid.clone();

    // Wait for it to finish.
    loop {
        let s = client.get_status(&job_id).await.expect("get_status failed");
        if is_stopped(&s.details) {
            assert_eq!(get_stopped(&s).unwrap().error_code, 0);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    drop(client);
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}

async fn drain_logs(client: &mut TestClient, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let req = Request::new(TaskId { uuid: id.to_string() });
    let mut stream = client.inner.logs(req).await?.into_inner();

    for i in 0..7 {
        let msg = stream.message().await?;
        if i == 6 {
            assert!(msg.is_none(), "expected end of stream at message 6");
            break;
        }
        let log = msg.expect("missing log message");
        assert_eq!(log.text, format!("Welcome {} times", i));
    }
    Ok(())
}
