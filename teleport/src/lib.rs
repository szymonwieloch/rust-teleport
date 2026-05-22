//! # teleport — Secure remote command executor and manager
//!
//! A gRPC-based server and CLI client for executing, monitoring, and managing
//! remote processes with resource limits and TLS authentication.
//!
//! ## Modules
//!
//! - [`job`] — Process lifecycle management, log capture, and resource monitoring.
//! - [`jobs`] — Thread-safe collection of all running and completed jobs.
//! - [`protocol`] — Generated protobuf types and gRPC service definitions.
//! - [`service`] — gRPC service implementation (the server-side RPC handlers).
//! - [`utils`] — Shared utilities for configuration file discovery.

pub mod job;
pub mod jobs;
pub mod protocol;
pub mod service;
pub mod utils;
