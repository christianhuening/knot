//! Hourly GC of doc_snapshots + the matching range of doc_updates.
//!
//! Per spec §5.4: after a snapshot at seq S, delete `doc_updates WHERE seq
//! <= S - retention_K` (retention_K = 2 * KNOT_SNAPSHOT_EVERY_N). Snapshot
//! retention is "keep last 5 + 1/day for 30 days".
//!
//! This task scans all docs that have at least one snapshot row and runs
//! both GCs. v0.1's workload is small; a full scan is fine.

use std::sync::Arc;
use std::time::Duration;

use knot_storage::{SnapshotStore, UpdatesStore};
use sqlx::PgPool;

pub fn spawn(
    pool: PgPool,
    snapshots: Arc<dyn SnapshotStore>,
    updates: Arc<dyn UpdatesStore>,
    snapshot_every_n: u32,
) {
    tokio::spawn(async move {
        let retention_k: i64 = i64::from(snapshot_every_n) * 2;
        loop {
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
            let docs = match sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT DISTINCT doc_id FROM doc_snapshots",
            )
            .fetch_all(&pool)
            .await
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error=?e, "gc: enumerate docs failed");
                    continue;
                }
            };
            for doc_id in docs {
                if let Ok(Some(snap)) = snapshots.latest(doc_id).await {
                    let cutoff = snap.snapshot_seq - retention_k;
                    if cutoff > 0
                        && let Err(e) = updates.delete_up_to(doc_id, cutoff).await
                    {
                        tracing::warn!(error=?e, %doc_id, "gc updates failed");
                    }
                }
                if let Err(e) = snapshots.gc(doc_id, 5, 30).await {
                    tracing::warn!(error=?e, %doc_id, "gc snapshots failed");
                }
            }
        }
    });
}
