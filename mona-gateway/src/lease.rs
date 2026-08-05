use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LeaseError {
    /// Reserved for multi-replica Postgres lease denials.
    #[error("failed to acquire writer lease for {db_id}: {reason}")]
    #[allow(dead_code)]
    Denied { db_id: String, reason: String },
}

/// Guard held while a tenant's SlateDB handles may be open for writes.
/// Dropping releases the lease (no-op for [`LocalLease`]).
pub struct LeaseGuard {
    #[allow(dead_code)]
    pub db_id: String,
}

impl Drop for LeaseGuard {
    fn drop(&mut self) {
        // Multi-replica follow-up: heartbeat cancel + release row in Postgres.
    }
}

#[async_trait]
pub trait WriterLease: Send + Sync {
    async fn acquire(&self, db_id: &str) -> Result<LeaseGuard, LeaseError>;
}

/// Single-replica stub: always grants. Replace with Postgres leases for multi-replica fleets.
#[derive(Clone, Default)]
pub struct LocalLease;

#[async_trait]
impl WriterLease for LocalLease {
    async fn acquire(&self, db_id: &str) -> Result<LeaseGuard, LeaseError> {
        Ok(LeaseGuard {
            db_id: db_id.to_string(),
        })
    }
}
