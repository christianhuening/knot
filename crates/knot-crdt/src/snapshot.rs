//! Snapshot trigger logic. The actor calls `write_snapshot` after the
//! N-trigger fires (in the applied_rx arm) or the idle-trigger fires
//! (in a 1s tick).

use crate::engine::{DocHandle, Engine, EngineError};
use knot_storage::SnapshotStore;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub struct SnapshotPolicy {
    pub every_n: u32,
    pub idle: Duration,
}

pub struct SnapshotState {
    pub last_snapshot_seq: i64,
    pub updates_since_snapshot: u32,
    pub last_apply_at: Instant,
}

pub async fn write_snapshot(
    doc_id: Uuid,
    seq: i64,
    engine: &dyn Engine,
    doc: &DocHandle,
    store: &dyn SnapshotStore,
) -> Result<(), EngineError> {
    let state_bytes = engine.encode_state_as_update(doc, None)?;
    let sv = engine.encode_state_vector(doc)?;
    if let Err(e) = store.insert(doc_id, seq, &state_bytes, &sv).await {
        return Err(EngineError::Apply(e.to_string()));
    }
    Ok(())
}
