//! Thread-safe job registry.
//!
//! The [`Jobs`] type provides a concurrent-safe `HashMap` of all known jobs,
//! indexed by UUID. It supports insert, lookup, stop (which also removes),
//! and list operations.

use crate::job::{Job, JobStatusEnum};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Thread-safe collection of all running and recently-stopped jobs.
pub struct Jobs {
    pending: RwLock<HashMap<Uuid, Arc<Job>>>,
}

impl Jobs {
    pub fn new() -> Self {
        Jobs {
            pending: RwLock::new(HashMap::new()),
        }
    }

    /// Insert a job into the collection.
    pub async fn insert(&self, uuid: Uuid, job: Arc<Job>) {
        self.pending.write().await.insert(uuid, job);
    }

    /// Look up a job by UUID. Returns `None` if not found.
    pub async fn find(&self, uuid: &Uuid) -> Option<Arc<Job>> {
        self.pending.read().await.get(uuid).cloned()
    }

    /// Stop a job and remove it from the collection.
    /// Returns the final status, or `None` if the job was not found.
    pub async fn stop(&self, uuid: &Uuid) -> Option<JobStatusEnum> {
        let job = self.pending.write().await.remove(uuid)?;
        Some(job.stop().await)
    }

    /// Return a snapshot of all jobs and their current statuses.
    pub async fn list(&self) -> Vec<(Uuid, Arc<Job>)> {
        let pending = self.pending.read().await;
        pending
            .iter()
            .map(|(id, job)| (*id, Arc::clone(job)))
            .collect()
    }

    /// Stop all jobs in the collection. Called on server shutdown.
    #[allow(dead_code)]
    pub async fn kill_all(&self) {
        let jobs: Vec<Arc<Job>> = {
            let mut pending = self.pending.write().await;
            pending.drain().map(|(_, job)| job).collect()
        };
        for job in jobs {
            job.stop().await;
        }
    }
}
