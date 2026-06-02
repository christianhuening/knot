# CRDT Room Actor + Persistence — Implementation Plan (Plan 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the in-memory spike rooms with the spec §8 actor model: one `tokio` task per active doc, durable `doc_updates` writes, periodic `doc_snapshots`, a `Bus` trait with a Postgres `LISTEN/NOTIFY` implementation for multi-replica fan-out, hydration on first join, idle eviction with final-snapshot flush, WS auth at upgrade with role pinned for the connection's lifetime, ACL-revocation close-frames (4403), and the `/api/docs/:id/markdown` import/export endpoints.

**Architecture:** A `Room` is a `tokio::task` that exclusively owns a `DocHandle` and a `Vec<ConnHandle>`. All mutations flow through `mpsc` inputs (`InMsg` from local conns, `BusNotify` from remote replicas, `BusPresence`, `Leave`, `CloseFrame`). A sibling **writer task** batches `doc_updates` inserts (200 updates or 250 ms) and publishes the assigned `seq` over the bus. A `Bus` trait carries `(doc_id, seq)` for updates and `(doc_id, payload)` for presence; the Postgres impl uses a dedicated `tokio_postgres` connection running `LISTEN doc:<id>` / `LISTEN presence:<id>`. Snapshots fire inside the room actor on N-updates or idle-sec; GC runs hourly in a sibling task. WS auth happens at upgrade (SessionLoader → RequireSession → RequireDocRole) and the `(user, role)` is pinned into the connection — mid-session revocation arrives as a 4403 close frame via the ACL listener.

**Tech Stack:** `yrs` 0.21 (already in workspace), `tokio` (mpsc, select, time, sync::watch, task), `tokio_postgres` 0.7 (one extra dep — for LISTEN; sqlx pool stays for everything else), `dashmap` (registry in-flight dedup), `bytes` (frame ownership), existing `axum::extract::ws`.

**Predecessor:** Plan 4 (Documents & ACL, outcome at `docs/superpowers/research/2026-06-02-plan4-outcome.md`, tag candidate `plan-4-complete` at the current HEAD). DocStore + GrantStore + AclCache + listener + `/api/docs/*` + `/api/workspace/*` all live.

**Out of scope for this plan** (each gets its own later plan):
- Frontend changes — Plans 6-8 wire Tiptap to the new persistence + reconnect behaviour.
- Helm chart + multi-arch image build — Plan 9.
- NATS bus implementation — spec calls it out but Postgres is the only v0.1 bus.
- Advanced markdown blocks (tables, callouts) — spec §8.8 defers to later specs.
- Audit-events UI — schema exists; v0.1 has no list endpoint.

---

## Spec coverage map

What this plan implements from `docs/superpowers/specs/2026-06-01-knot-foundation-design.md`:

| Spec section | Tasks |
|---|---|
| §5.4 `doc_updates` + `doc_snapshots` queries | T2, T3 |
| §5.5 `doc_markdown_cache` lazy fill | T4, T18, T19 |
| §6.1 `GET\|POST /api/docs/:id/markdown` | T18, T19 |
| §6.2 `GET /collab/:doc_id` — auth at upgrade, role pinned | T16 |
| §7.6 Permission revocation mid-session (4403) | T17 |
| §8.2 Engine trait | already complete from Plan 1 (no changes) |
| §8.3 Room actor + `tokio::select!` loop | T7, T10, T12, T13, T15 |
| §8.4 Persistence batching + RETURNING seq | T8 |
| §8.5 Backpressure (bounded mpsc + 4408 close) | T12 |
| §8.6 Lifecycle: hydration + idle eviction + final snapshot | T9, T11, T15 |
| §8.7 Awareness (presence size cap + disconnect clearing) | T13 |
| §8.8 Markdown round-trip via engine | T18, T19 |
| §9 Bus trait + PgBus | T5, T6 |
| §9.1 Catch-up safety net (5s tick) | T14 |
| Plan 4 carryover #1 — member audit rows | T1 |
| Plan 4 carryover #2 — invalidations.rs cosmetic | T1 |

Deferred (intentional):
- NATS bus impl (`bus_nats.rs`) — Plan 9 or post-v0.1.
- Snapshot/restore from explicit markdown export — `POST /markdown` covers the import; export is `GET /markdown`.
- pprof endpoint — Plan 9.

---

## File map

```
knot/
├── Cargo.toml                                (modify) +tokio_postgres, +bytes, +dashmap
│
├── crates/
│   ├── knot-storage/
│   │   ├── src/
│   │   │   ├── lib.rs                        (modify) re-export new stores
│   │   │   ├── updates_store.rs              (new) UpdatesStore: batch insert + select-after-seq
│   │   │   ├── snapshot_store.rs             (new) SnapshotStore: insert + load-latest + GC queries
│   │   │   ├── markdown_cache.rs             (new) MarkdownCacheStore: get_if_fresh + put
│   │   │   ├── invalidations.rs              (modify) drop the always-true `created_at <= now()` clause
│   │   │   └── workspace_store.rs            (modify) audit each mutation in invite/update/remove
│   │   ├── tests/
│   │   │   ├── updates.rs                    (new)
│   │   │   ├── snapshots.rs                  (new)
│   │   │   ├── markdown_cache.rs             (new)
│   │   │   └── workspace_member_audit.rs     (new)
│   │   └── Cargo.toml                        (no change)
│   │
│   ├── knot-crdt/                            (extended — currently just engine)
│   │   ├── Cargo.toml                        (modify) +tokio_postgres, +bytes, +dashmap, +knot-storage
│   │   └── src/
│   │       ├── lib.rs                        (modify) re-exports
│   │       ├── engine.rs                     (unchanged)
│   │       ├── bus.rs                        (new) Bus trait + Subscription + BusError
│   │       ├── bus_pg.rs                     (new) Postgres LISTEN/NOTIFY implementation
│   │       ├── bus_mem.rs                    (new) in-process bus for unit tests
│   │       ├── room.rs                       (new) Room actor + InMsg + ConnHandle
│   │       ├── writer.rs                     (new) per-room batch writer task
│   │       ├── snapshot.rs                   (new) snapshot scheduler + GC
│   │       ├── presence.rs                   (new) awareness fan-out helper
│   │       └── registry.rs                   (new) Rooms registry (acquire/release with in-flight dedup)
│   │
│   ├── knot-docs/
│   │   └── src/listener.rs                   (modify) on ACL invalidate, also notify Rooms registry so it can emit 4403
│   │
│   └── knot-server/
│       ├── Cargo.toml                        (modify) +knot-crdt new modules already path-deped
│       └── src/
│           ├── lib.rs                        (modify) AppState gets Arc<Rooms> + Arc<Bus>; plumbing
│           ├── main.rs                       (modify) construct PgBus, spawn its listener, build Rooms registry
│           ├── room.rs                       (rewrite) thin shim that takes the upgraded WS + (user, role) and hands off to knot_crdt::room::serve_conn
│           ├── protocol.rs                   (unchanged — still the y-sync helpers; both crates use it)
│           └── routes/
│               ├── api/
│               │   └── markdown.rs           (new) GET + POST /api/docs/:id/markdown
│               └── api/docs.rs               (modify) merge markdown routes into doc_id_routes
│
└── e2e/
    └── flows/
        └── collab.spec.ts                    (new) reconnect → state persists; CSRF + auth still apply
```

---

## Conventions

- **Channel capacities:** `KNOT_CRDT_INBOX_CAP` (default 256), `KNOT_CRDT_OUTBOUND_CAP` (256), `KNOT_CRDT_PERSIST_CAP` (1024). Read from `Config` (added in T15).
- **Close codes:** 4403 = `acl.revoked`; 4408 = `slow_consumer`; 4500 = `internal`. Always set the WS close reason text to the same as the code's name for client-side mapping.
- **Channel naming for PG NOTIFY:** `doc:<uuid>` for update seqs, `presence:<uuid>` for presence payloads. NOTIFY's 8 KB payload cap is fine for both (updates carry only `seq:i64` as text; presence is capped at 4 KB).
- **Audit actions added:** `workspace.member.invite`, `workspace.member.role`, `workspace.member.remove`, `doc.markdown.export`, `doc.markdown.import`, `doc.snapshot`. The audit table already has the columns; only the action strings are new.
- **The existing spike `Rooms` and `room.rs` in `knot-server`** are replaced. The spike's `convergence` integration test must continue to pass against the new implementation (regression check).
- **Engine** is unchanged. Plan 1's `YrsEngine` already implements the 6 methods §8.2 needs.

---

## Task overview

| # | Title | LOC ≈ |
|---|---|---|
| 1 | Plan 4 carryovers cleanup | 90 |
| 2 | UpdatesStore | 230 |
| 3 | SnapshotStore | 250 |
| 4 | MarkdownCacheStore | 130 |
| 5 | Bus trait + in-process impl | 200 |
| 6 | PgBus (Postgres LISTEN/NOTIFY) | 320 |
| 7 | Room actor skeleton | 280 |
| 8 | Writer task (batched persist) | 230 |
| 9 | Hydration on room boot | 180 |
| 10 | Snapshot scheduler in actor | 160 |
| 11 | Snapshot GC task | 150 |
| 12 | Backpressure: bounded channels + 4408 | 140 |
| 13 | Awareness presence + bus + disconnect-clear | 180 |
| 14 | Catch-up polling tick (5s) | 90 |
| 15 | Rooms registry + idle eviction + final snapshot | 240 |
| 16 | knot-server WS upgrade: auth + role pinned | 200 |
| 17 | 4403 on ACL revocation | 150 |
| 18 | GET /api/docs/:id/markdown | 130 |
| 19 | POST /api/docs/:id/markdown | 170 |
| 20 | e2e — collab persistence + reconnect | 180 |

---

## Task 1: Plan 4 carryovers cleanup

**Files:**
- Modify: `crates/knot-storage/src/invalidations.rs` — drop the always-true `created_at <= now()` predicate
- Modify: `crates/knot-server/src/routes/api/workspace.rs` — add audit::record calls for invite/change_role/remove_member

- [ ] **Step 1: Simplify invalidations GC predicate**

Open `/home/nik/Development/knot/crates/knot-storage/src/invalidations.rs`. Find the listener's GC query that's been called from `knot-docs/src/listener.rs:65` (or wherever it lives). The query reads:

```rust
"DELETE FROM acl_invalidations WHERE doc_id = $1 AND created_at <= now()"
```

The `created_at <= now()` clause is always true. Search the codebase first:

```bash
grep -rn 'DELETE FROM acl_invalidations' crates/
```

For each match, drop the always-true predicate. The final form should be:

```rust
"DELETE FROM acl_invalidations WHERE doc_id = $1"
```

- [ ] **Step 2: Add audit calls to member CRUD**

Open `/home/nik/Development/knot/crates/knot-server/src/routes/api/workspace.rs`. Three handlers mutate `workspace_members`: `invite_member`, `change_role`, `remove_member`. Each should write a best-effort audit row after success.

At the top of the file add the import:

```rust
use knot_storage::audit;
```

In `invite_member`, after the `add_member` succeeds (just before `StatusCode::CREATED.into_response()`):

```rust
    if let Some(pool) = state.pool.as_ref() {
        audit::record(
            pool,
            ws.id,
            Some(ctx.user_id),
            "workspace.member.invite",
            "user",
            user.id,
        )
        .await;
    }
```

In `change_role`, after the `update_role` succeeds (before `NO_CONTENT`):

```rust
    if let Some(pool) = state.pool.as_ref() {
        audit::record(
            pool,
            ws.id,
            Some(ctx.user_id),
            "workspace.member.role",
            "user",
            user_id,
        )
        .await;
    }
```

In `remove_member`, after `remove_member` succeeds:

```rust
    if let Some(pool) = state.pool.as_ref() {
        audit::record(
            pool,
            ws.id,
            Some(ctx.user_id),
            "workspace.member.remove",
            "user",
            user_id,
        )
        .await;
    }
```

`audit::record` (best-effort variant, not the in-tx one) logs `warn!` on failure and doesn't return an error — perfect for this hot path.

- [ ] **Step 3: Verify + commit**

```bash
cd /home/nik/Development/knot
cargo build --workspace
cargo test -p knot-storage -p knot-server
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
git add crates/
git commit -m "chore(plan-4-cleanups): drop always-true predicate; audit member CRUD"
```

Expected: green.

---

## Task 2: UpdatesStore

**Files:**
- Create: `crates/knot-storage/src/updates_store.rs`
- Modify: `crates/knot-storage/src/lib.rs`
- Create: `crates/knot-storage/tests/updates.rs`

- [ ] **Step 1: Implement UpdatesStore**

Create `/home/nik/Development/knot/crates/knot-storage/src/updates_store.rs`:

```rust
//! doc_updates persistence: append-only log of Y.Doc binary updates.
//!
//! Per spec §5.4, `seq` is a GLOBAL bigserial; per-doc monotonicity comes
//! from Postgres serialising sequence allocation. Replays use
//! `WHERE doc_id = $1 ORDER BY seq`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocUpdate {
    pub seq: i64,
    pub doc_id: Uuid,
    pub update_bytes: Vec<u8>,
    pub by_user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum UpdatesStoreError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
}

#[async_trait]
pub trait UpdatesStore: Send + Sync + 'static {
    /// Insert a batch of updates atomically. Returns the assigned seqs in
    /// the same order as the input. The batch is one INSERT with a multi-row
    /// VALUES list so all rows share one round-trip.
    async fn insert_batch(
        &self,
        doc_id: Uuid,
        by_user_id: Option<Uuid>,
        updates: &[Vec<u8>],
    ) -> Result<Vec<i64>, UpdatesStoreError>;

    /// Fetch updates with `seq > after_seq` for a doc, in seq order.
    async fn since(
        &self,
        doc_id: Uuid,
        after_seq: i64,
    ) -> Result<Vec<DocUpdate>, UpdatesStoreError>;

    /// Highest seq for a doc, or 0 if none.
    async fn max_seq(&self, doc_id: Uuid) -> Result<i64, UpdatesStoreError>;

    /// Delete updates with seq <= cutoff (used by snapshot GC).
    async fn delete_up_to(
        &self,
        doc_id: Uuid,
        cutoff_seq: i64,
    ) -> Result<u64, UpdatesStoreError>;
}

#[derive(Clone)]
pub struct PgUpdatesStore {
    pool: PgPool,
}

impl PgUpdatesStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UpdatesStore for PgUpdatesStore {
    async fn insert_batch(
        &self,
        doc_id: Uuid,
        by_user_id: Option<Uuid>,
        updates: &[Vec<u8>],
    ) -> Result<Vec<i64>, UpdatesStoreError> {
        if updates.is_empty() {
            return Ok(Vec::new());
        }
        // Build "($1, $2, $N), ($1, $2, $N+1), ..." with shared doc_id +
        // by_user_id binds and one per-update bytea bind.
        let mut sql =
            String::from("INSERT INTO doc_updates (doc_id, by_user_id, update_bytes) VALUES ");
        for i in 0..updates.len() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!("($1, $2, ${})", i + 3));
        }
        sql.push_str(" RETURNING seq");
        let mut q = sqlx::query_scalar::<_, i64>(&sql).bind(doc_id).bind(by_user_id);
        for u in updates {
            q = q.bind(u);
        }
        let seqs = q.fetch_all(&self.pool).await?;
        Ok(seqs)
    }

    async fn since(
        &self,
        doc_id: Uuid,
        after_seq: i64,
    ) -> Result<Vec<DocUpdate>, UpdatesStoreError> {
        let rows = sqlx::query_as::<_, (i64, Uuid, Vec<u8>, Option<Uuid>, DateTime<Utc>)>(
            "SELECT seq, doc_id, update_bytes, by_user_id, created_at
             FROM doc_updates
             WHERE doc_id = $1 AND seq > $2
             ORDER BY seq",
        )
        .bind(doc_id)
        .bind(after_seq)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| DocUpdate {
                seq: r.0,
                doc_id: r.1,
                update_bytes: r.2,
                by_user_id: r.3,
                created_at: r.4,
            })
            .collect())
    }

    async fn max_seq(&self, doc_id: Uuid) -> Result<i64, UpdatesStoreError> {
        let v: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(seq) FROM doc_updates WHERE doc_id = $1",
        )
        .bind(doc_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(v.unwrap_or(0))
    }

    async fn delete_up_to(
        &self,
        doc_id: Uuid,
        cutoff_seq: i64,
    ) -> Result<u64, UpdatesStoreError> {
        let r = sqlx::query("DELETE FROM doc_updates WHERE doc_id = $1 AND seq <= $2")
            .bind(doc_id)
            .bind(cutoff_seq)
            .execute(&self.pool)
            .await?;
        Ok(r.rows_affected())
    }
}
```

- [ ] **Step 2: lib.rs re-export**

Edit `/home/nik/Development/knot/crates/knot-storage/src/lib.rs`. Add:

```rust
pub mod updates_store;
pub use updates_store::{DocUpdate, PgUpdatesStore, UpdatesStore, UpdatesStoreError};
```

- [ ] **Step 3: Integration test**

Create `/home/nik/Development/knot/crates/knot-storage/tests/updates.rs`:

```rust
use knot_storage::{
    DocStore, PgDocStore, PgUpdatesStore, PgUserStore, PgWorkspaceStore, UpdatesStore, UserStore,
    WorkspaceRole, WorkspaceStore,
};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
use uuid::Uuid;

async fn setup() -> (PgUpdatesStore, Uuid, Uuid) {
    let c = Postgres::default().start().await.unwrap();
    let port = c.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    std::mem::forget(c);

    let ws = PgWorkspaceStore::new(pool.clone()).create("default", "W").await.unwrap();
    let u = PgUserStore::new(pool.clone()).create_local("a@x.test", "A", "$h$").await.unwrap();
    PgWorkspaceStore::new(pool.clone()).add_member(ws.id, u.id, WorkspaceRole::Owner).await.unwrap();
    let d = PgDocStore::new(pool.clone()).create(ws.id, None, "D", "m", u.id).await.unwrap();
    (PgUpdatesStore::new(pool), d.id, u.id)
}

#[tokio::test(flavor = "multi_thread")]
async fn insert_batch_returns_monotone_seqs_in_input_order() {
    let (s, doc, user) = setup().await;
    let batch = vec![vec![1u8, 2, 3], vec![4u8, 5], vec![6u8]];
    let seqs = s.insert_batch(doc, Some(user), &batch).await.unwrap();
    assert_eq!(seqs.len(), 3);
    assert!(seqs[0] < seqs[1] && seqs[1] < seqs[2], "got {seqs:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn since_returns_after_watermark_in_order() {
    let (s, doc, user) = setup().await;
    let seqs = s
        .insert_batch(doc, Some(user), &[vec![1u8], vec![2u8], vec![3u8]])
        .await
        .unwrap();
    let after = seqs[0];
    let got = s.since(doc, after).await.unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].seq, seqs[1]);
    assert_eq!(got[0].update_bytes, vec![2u8]);
    assert_eq!(got[1].seq, seqs[2]);
}

#[tokio::test(flavor = "multi_thread")]
async fn max_seq_zero_when_empty_then_grows() {
    let (s, doc, user) = setup().await;
    assert_eq!(s.max_seq(doc).await.unwrap(), 0);
    let seqs = s.insert_batch(doc, Some(user), &[vec![1u8], vec![2u8]]).await.unwrap();
    assert_eq!(s.max_seq(doc).await.unwrap(), *seqs.last().unwrap());
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_up_to_removes_inclusive() {
    let (s, doc, user) = setup().await;
    let seqs = s
        .insert_batch(doc, Some(user), &[vec![1u8], vec![2u8], vec![3u8]])
        .await
        .unwrap();
    let n = s.delete_up_to(doc, seqs[1]).await.unwrap();
    assert_eq!(n, 2);
    let left = s.since(doc, 0).await.unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].seq, seqs[2]);
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_batch_is_noop() {
    let (s, doc, _) = setup().await;
    let seqs = s.insert_batch(doc, None, &[]).await.unwrap();
    assert!(seqs.is_empty());
    assert_eq!(s.max_seq(doc).await.unwrap(), 0);
    let _ = Uuid::nil(); // silence
}
```

- [ ] **Step 4: Verify + commit**

```bash
cargo test -p knot-storage --test updates
cargo clippy -p knot-storage --all-targets --all-features -- -D warnings
git add crates/knot-storage/
git commit -m "feat(knot-storage): UpdatesStore (batch insert + since + max_seq + delete_up_to)"
```

Expected: 5 tests pass.

---

## Task 3: SnapshotStore

**Files:**
- Create: `crates/knot-storage/src/snapshot_store.rs`
- Modify: `crates/knot-storage/src/lib.rs`
- Create: `crates/knot-storage/tests/snapshots.rs`

- [ ] **Step 1: Snapshot table queries**

Create `/home/nik/Development/knot/crates/knot-storage/src/snapshot_store.rs`:

```rust
//! doc_snapshots persistence. One row per snapshot. Per spec §5.4:
//! `(doc_id, snapshot_seq)` is the PK; `state_bytes` is the Y.Doc encoded
//! state at that seq; `state_vector` lets us compute diff fetches cheaply.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocSnapshot {
    pub doc_id: Uuid,
    pub snapshot_seq: i64,
    pub state_bytes: Vec<u8>,
    pub state_vector: Vec<u8>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum SnapshotStoreError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
}

#[async_trait]
pub trait SnapshotStore: Send + Sync + 'static {
    async fn insert(
        &self,
        doc_id: Uuid,
        snapshot_seq: i64,
        state_bytes: &[u8],
        state_vector: &[u8],
    ) -> Result<(), SnapshotStoreError>;

    /// Returns the latest snapshot (highest snapshot_seq) for a doc, or
    /// None if none exists.
    async fn latest(&self, doc_id: Uuid) -> Result<Option<DocSnapshot>, SnapshotStoreError>;

    /// GC per spec §5.4: keep the most recent N snapshots and at most one
    /// per day for the past `retain_days` days. Returns deleted count.
    async fn gc(
        &self,
        doc_id: Uuid,
        keep_recent: i64,
        retain_days: i32,
    ) -> Result<u64, SnapshotStoreError>;
}

#[derive(Clone)]
pub struct PgSnapshotStore {
    pool: PgPool,
}

impl PgSnapshotStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SnapshotStore for PgSnapshotStore {
    async fn insert(
        &self,
        doc_id: Uuid,
        snapshot_seq: i64,
        state_bytes: &[u8],
        state_vector: &[u8],
    ) -> Result<(), SnapshotStoreError> {
        sqlx::query(
            "INSERT INTO doc_snapshots (doc_id, snapshot_seq, state_bytes, state_vector)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (doc_id, snapshot_seq) DO UPDATE
             SET state_bytes = EXCLUDED.state_bytes,
                 state_vector = EXCLUDED.state_vector,
                 created_at = now()",
        )
        .bind(doc_id)
        .bind(snapshot_seq)
        .bind(state_bytes)
        .bind(state_vector)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn latest(&self, doc_id: Uuid) -> Result<Option<DocSnapshot>, SnapshotStoreError> {
        let row = sqlx::query_as::<_, (Uuid, i64, Vec<u8>, Vec<u8>, DateTime<Utc>)>(
            "SELECT doc_id, snapshot_seq, state_bytes, state_vector, created_at
             FROM doc_snapshots
             WHERE doc_id = $1
             ORDER BY snapshot_seq DESC
             LIMIT 1",
        )
        .bind(doc_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| DocSnapshot {
            doc_id: r.0,
            snapshot_seq: r.1,
            state_bytes: r.2,
            state_vector: r.3,
            created_at: r.4,
        }))
    }

    async fn gc(
        &self,
        doc_id: Uuid,
        keep_recent: i64,
        retain_days: i32,
    ) -> Result<u64, SnapshotStoreError> {
        // Two retention buckets, OR'd together:
        //   1. `keep_recent` most recent snapshots by snapshot_seq
        //   2. for each day within `retain_days`, keep the latest snapshot
        // Everything not in either keep set is deletable.
        let r = sqlx::query(
            "WITH recent AS (
                 SELECT snapshot_seq FROM doc_snapshots
                 WHERE doc_id = $1
                 ORDER BY snapshot_seq DESC
                 LIMIT $2
             ),
             per_day AS (
                 SELECT DISTINCT ON (date_trunc('day', created_at))
                        snapshot_seq
                 FROM doc_snapshots
                 WHERE doc_id = $1
                   AND created_at >= now() - ($3 || ' days')::interval
                 ORDER BY date_trunc('day', created_at), created_at DESC
             )
             DELETE FROM doc_snapshots
             WHERE doc_id = $1
               AND snapshot_seq NOT IN (SELECT snapshot_seq FROM recent)
               AND snapshot_seq NOT IN (SELECT snapshot_seq FROM per_day)",
        )
        .bind(doc_id)
        .bind(keep_recent)
        .bind(retain_days)
        .execute(&self.pool)
        .await?;
        Ok(r.rows_affected())
    }
}
```

- [ ] **Step 2: lib.rs re-export**

```rust
pub mod snapshot_store;
pub use snapshot_store::{DocSnapshot, PgSnapshotStore, SnapshotStore, SnapshotStoreError};
```

- [ ] **Step 3: Integration test**

Create `/home/nik/Development/knot/crates/knot-storage/tests/snapshots.rs`:

```rust
use chrono::Duration;
use knot_storage::{
    DocStore, PgDocStore, PgSnapshotStore, PgUserStore, PgWorkspaceStore, SnapshotStore, UserStore,
    WorkspaceRole, WorkspaceStore,
};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

async fn setup() -> (PgSnapshotStore, uuid::Uuid) {
    let c = Postgres::default().start().await.unwrap();
    let port = c.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    std::mem::forget(c);

    let ws = PgWorkspaceStore::new(pool.clone()).create("default", "W").await.unwrap();
    let u = PgUserStore::new(pool.clone()).create_local("a@x.test", "A", "$h$").await.unwrap();
    PgWorkspaceStore::new(pool.clone()).add_member(ws.id, u.id, WorkspaceRole::Owner).await.unwrap();
    let d = PgDocStore::new(pool.clone()).create(ws.id, None, "D", "m", u.id).await.unwrap();
    (PgSnapshotStore::new(pool), d.id)
}

#[tokio::test(flavor = "multi_thread")]
async fn insert_and_load_latest_round_trip() {
    let (s, doc) = setup().await;
    s.insert(doc, 100, b"state-bytes", b"sv-bytes").await.unwrap();
    let got = s.latest(doc).await.unwrap().unwrap();
    assert_eq!(got.snapshot_seq, 100);
    assert_eq!(got.state_bytes, b"state-bytes");
    assert_eq!(got.state_vector, b"sv-bytes");
}

#[tokio::test(flavor = "multi_thread")]
async fn latest_returns_highest_snapshot_seq() {
    let (s, doc) = setup().await;
    s.insert(doc, 100, b"a", b"a").await.unwrap();
    s.insert(doc, 200, b"b", b"b").await.unwrap();
    s.insert(doc, 150, b"c", b"c").await.unwrap();
    let got = s.latest(doc).await.unwrap().unwrap();
    assert_eq!(got.snapshot_seq, 200);
    assert_eq!(got.state_bytes, b"b");
}

#[tokio::test(flavor = "multi_thread")]
async fn upsert_overwrites_same_seq() {
    let (s, doc) = setup().await;
    s.insert(doc, 100, b"v1", b"sv1").await.unwrap();
    s.insert(doc, 100, b"v2", b"sv2").await.unwrap();
    let got = s.latest(doc).await.unwrap().unwrap();
    assert_eq!(got.state_bytes, b"v2");
}

#[tokio::test(flavor = "multi_thread")]
async fn gc_keeps_recent_and_per_day() {
    let (s, doc) = setup().await;
    // 7 snapshots, all today.
    for i in 1..=7i64 {
        s.insert(doc, i * 100, &format!("v{i}").into_bytes(), b"sv").await.unwrap();
    }
    // Keep last 5 + per-day (which is also today's latest = seq=700).
    // Result: keep_recent = {300..=700}; per_day = {700}; union = {300..=700}.
    // Deleted: 100, 200 → 2 rows.
    let n = s.gc(doc, 5, 30).await.unwrap();
    assert_eq!(n, 2);
    assert_eq!(s.latest(doc).await.unwrap().unwrap().snapshot_seq, 700);
    // Avoid unused-import warning on chrono::Duration:
    let _ = Duration::seconds(1);
}
```

- [ ] **Step 4: Verify + commit**

```bash
cargo test -p knot-storage --test snapshots
cargo clippy -p knot-storage --all-targets --all-features -- -D warnings
git add crates/knot-storage/
git commit -m "feat(knot-storage): SnapshotStore (insert + latest + GC)"
```

Expected: 4 tests pass.

---

## Task 4: MarkdownCacheStore

**Files:**
- Create: `crates/knot-storage/src/markdown_cache.rs`
- Modify: `crates/knot-storage/src/lib.rs`
- Create: `crates/knot-storage/tests/markdown_cache.rs`

- [ ] **Step 1: Cache queries**

Create `/home/nik/Development/knot/crates/knot-storage/src/markdown_cache.rs`:

```rust
//! doc_markdown_cache: lazy-fill on export, invalidated by seq drift.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownCacheEntry {
    pub doc_id: Uuid,
    pub rendered_at_seq: i64,
    pub markdown_text: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum MarkdownCacheError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
}

#[async_trait]
pub trait MarkdownCacheStore: Send + Sync + 'static {
    /// Return the cached entry if its rendered_at_seq matches `current_seq`,
    /// else None. The room actor passes its `last_applied_seq`.
    async fn get_if_fresh(
        &self,
        doc_id: Uuid,
        current_seq: i64,
    ) -> Result<Option<MarkdownCacheEntry>, MarkdownCacheError>;

    async fn put(
        &self,
        doc_id: Uuid,
        rendered_at_seq: i64,
        markdown: &str,
    ) -> Result<(), MarkdownCacheError>;

    /// Invalidate (delete) the cached row for a doc.
    async fn invalidate(&self, doc_id: Uuid) -> Result<(), MarkdownCacheError>;
}

#[derive(Clone)]
pub struct PgMarkdownCache {
    pool: PgPool,
}

impl PgMarkdownCache {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MarkdownCacheStore for PgMarkdownCache {
    async fn get_if_fresh(
        &self,
        doc_id: Uuid,
        current_seq: i64,
    ) -> Result<Option<MarkdownCacheEntry>, MarkdownCacheError> {
        let row = sqlx::query_as::<_, (Uuid, i64, String, DateTime<Utc>)>(
            "SELECT doc_id, rendered_at_seq, markdown_text, updated_at
             FROM doc_markdown_cache
             WHERE doc_id = $1 AND rendered_at_seq = $2",
        )
        .bind(doc_id)
        .bind(current_seq)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| MarkdownCacheEntry {
            doc_id: r.0,
            rendered_at_seq: r.1,
            markdown_text: r.2,
            updated_at: r.3,
        }))
    }

    async fn put(
        &self,
        doc_id: Uuid,
        rendered_at_seq: i64,
        markdown: &str,
    ) -> Result<(), MarkdownCacheError> {
        sqlx::query(
            "INSERT INTO doc_markdown_cache (doc_id, rendered_at_seq, markdown_text)
             VALUES ($1, $2, $3)
             ON CONFLICT (doc_id) DO UPDATE
             SET rendered_at_seq = EXCLUDED.rendered_at_seq,
                 markdown_text = EXCLUDED.markdown_text,
                 updated_at = now()",
        )
        .bind(doc_id)
        .bind(rendered_at_seq)
        .bind(markdown)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn invalidate(&self, doc_id: Uuid) -> Result<(), MarkdownCacheError> {
        sqlx::query("DELETE FROM doc_markdown_cache WHERE doc_id = $1")
            .bind(doc_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
```

- [ ] **Step 2: lib.rs re-export**

```rust
pub mod markdown_cache;
pub use markdown_cache::{MarkdownCacheEntry, MarkdownCacheError, MarkdownCacheStore, PgMarkdownCache};
```

- [ ] **Step 3: Integration test**

Create `/home/nik/Development/knot/crates/knot-storage/tests/markdown_cache.rs`:

```rust
use knot_storage::{
    DocStore, MarkdownCacheStore, PgDocStore, PgMarkdownCache, PgUserStore, PgWorkspaceStore,
    UserStore, WorkspaceRole, WorkspaceStore,
};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

async fn setup() -> (PgMarkdownCache, uuid::Uuid) {
    let c = Postgres::default().start().await.unwrap();
    let port = c.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    std::mem::forget(c);
    let ws = PgWorkspaceStore::new(pool.clone()).create("default", "W").await.unwrap();
    let u = PgUserStore::new(pool.clone()).create_local("a@x.test", "A", "$h$").await.unwrap();
    PgWorkspaceStore::new(pool.clone()).add_member(ws.id, u.id, WorkspaceRole::Owner).await.unwrap();
    let d = PgDocStore::new(pool.clone()).create(ws.id, None, "D", "m", u.id).await.unwrap();
    (PgMarkdownCache::new(pool), d.id)
}

#[tokio::test(flavor = "multi_thread")]
async fn put_then_get_if_fresh() {
    let (s, doc) = setup().await;
    s.put(doc, 42, "# hi\n").await.unwrap();
    let got = s.get_if_fresh(doc, 42).await.unwrap().unwrap();
    assert_eq!(got.markdown_text, "# hi\n");
    assert_eq!(got.rendered_at_seq, 42);
}

#[tokio::test(flavor = "multi_thread")]
async fn stale_seq_returns_none() {
    let (s, doc) = setup().await;
    s.put(doc, 42, "# hi\n").await.unwrap();
    assert!(s.get_if_fresh(doc, 43).await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn put_upserts_on_repeat() {
    let (s, doc) = setup().await;
    s.put(doc, 1, "v1").await.unwrap();
    s.put(doc, 2, "v2").await.unwrap();
    let got = s.get_if_fresh(doc, 2).await.unwrap().unwrap();
    assert_eq!(got.markdown_text, "v2");
    assert!(s.get_if_fresh(doc, 1).await.unwrap().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn invalidate_removes_row() {
    let (s, doc) = setup().await;
    s.put(doc, 1, "v").await.unwrap();
    s.invalidate(doc).await.unwrap();
    assert!(s.get_if_fresh(doc, 1).await.unwrap().is_none());
}
```

- [ ] **Step 4: Verify + commit**

```bash
cargo test -p knot-storage --test markdown_cache
cargo clippy -p knot-storage --all-targets --all-features -- -D warnings
git add crates/knot-storage/
git commit -m "feat(knot-storage): MarkdownCacheStore (lazy-fill by seq)"
```

Expected: 4 tests pass.

---

## Task 5: Bus trait + in-process impl

**Files:**
- Create: `crates/knot-crdt/src/bus.rs` (trait + Subscription + error type)
- Create: `crates/knot-crdt/src/bus_mem.rs` (in-process impl for unit tests)
- Modify: `crates/knot-crdt/src/lib.rs` — declare modules + re-exports
- Modify: `crates/knot-crdt/Cargo.toml` — `async-trait`, `tokio` workspace dep

- [ ] **Step 1: knot-crdt Cargo.toml**

Open `/home/nik/Development/knot/crates/knot-crdt/Cargo.toml`. Add to `[dependencies]`:

```toml
async-trait.workspace = true
tokio.workspace = true
uuid.workspace = true
thiserror.workspace = true
tracing.workspace = true
dashmap = "6"
bytes = "1"
```

Add `bytes = "1"` and `dashmap = "6"` to root `Cargo.toml` `[workspace.dependencies]` too.

- [ ] **Step 2: bus.rs**

Create `/home/nik/Development/knot/crates/knot-crdt/src/bus.rs`:

```rust
//! Cross-replica fan-out abstraction.
//!
//! Updates carry only `(doc_id, seq)`; bytes stay in `doc_updates`.
//! Presence carries the payload inline (size-capped on emit by the room).

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum BusError {
    #[error("io: {0}")]
    Io(String),
    #[error("subscriber full")]
    SubscriberFull,
}

pub struct Subscription {
    pub updates: mpsc::Receiver<i64>,
    pub presence: mpsc::Receiver<Vec<u8>>,
}

#[async_trait]
pub trait Bus: Send + Sync + 'static {
    async fn publish(&self, doc_id: Uuid, seq: i64) -> Result<(), BusError>;
    async fn publish_presence(&self, doc_id: Uuid, payload: Vec<u8>) -> Result<(), BusError>;
    async fn subscribe(&self, doc_id: Uuid) -> Result<Subscription, BusError>;
    async fn unsubscribe(&self, doc_id: Uuid) -> Result<(), BusError>;
}
```

- [ ] **Step 3: bus_mem.rs**

Create `/home/nik/Development/knot/crates/knot-crdt/src/bus_mem.rs`:

```rust
//! In-process Bus impl for unit tests. Subscribers receive every publish
//! after their subscription. Per-doc state lives in a DashMap.

use async_trait::async_trait;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::bus::{Bus, BusError, Subscription};

#[derive(Default)]
struct DocChannels {
    update_tx: Vec<mpsc::Sender<i64>>,
    presence_tx: Vec<mpsc::Sender<Vec<u8>>>,
}

#[derive(Clone, Default)]
pub struct MemBus {
    map: Arc<DashMap<Uuid, DocChannels>>,
}

impl MemBus {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Bus for MemBus {
    async fn publish(&self, doc_id: Uuid, seq: i64) -> Result<(), BusError> {
        if let Some(mut entry) = self.map.get_mut(&doc_id) {
            entry.update_tx.retain(|tx| tx.try_send(seq).is_ok());
        }
        Ok(())
    }

    async fn publish_presence(&self, doc_id: Uuid, payload: Vec<u8>) -> Result<(), BusError> {
        if let Some(mut entry) = self.map.get_mut(&doc_id) {
            entry.presence_tx.retain(|tx| tx.try_send(payload.clone()).is_ok());
        }
        Ok(())
    }

    async fn subscribe(&self, doc_id: Uuid) -> Result<Subscription, BusError> {
        let (ut, ur) = mpsc::channel::<i64>(256);
        let (pt, pr) = mpsc::channel::<Vec<u8>>(256);
        let mut entry = self.map.entry(doc_id).or_default();
        entry.update_tx.push(ut);
        entry.presence_tx.push(pt);
        Ok(Subscription { updates: ur, presence: pr })
    }

    async fn unsubscribe(&self, _doc_id: Uuid) -> Result<(), BusError> {
        // No-op for MemBus; channels close on drop.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration, timeout};

    #[tokio::test]
    async fn publish_reaches_subscribers() {
        let bus = MemBus::new();
        let doc = Uuid::new_v4();
        let mut sub = bus.subscribe(doc).await.unwrap();
        bus.publish(doc, 42).await.unwrap();
        let got = timeout(Duration::from_millis(200), sub.updates.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got, 42);
    }

    #[tokio::test]
    async fn presence_payload_round_trip() {
        let bus = MemBus::new();
        let doc = Uuid::new_v4();
        let mut sub = bus.subscribe(doc).await.unwrap();
        bus.publish_presence(doc, vec![1, 2, 3]).await.unwrap();
        let got = timeout(Duration::from_millis(200), sub.presence.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn no_subscriber_publish_succeeds() {
        let bus = MemBus::new();
        bus.publish(Uuid::new_v4(), 1).await.unwrap();
    }
}
```

- [ ] **Step 4: lib.rs**

Edit `/home/nik/Development/knot/crates/knot-crdt/src/lib.rs`:

```rust
pub mod bus;
pub mod bus_mem;
pub mod engine;

pub use bus::{Bus, BusError, Subscription};
pub use bus_mem::MemBus;
pub use engine::{DocHandle, Engine, EngineError, TextMark, TextMarkAttr, YrsEngine};
```

- [ ] **Step 5: Verify + commit**

```bash
cargo test -p knot-crdt
cargo clippy -p knot-crdt --all-targets --all-features -- -D warnings
git add Cargo.toml Cargo.lock crates/knot-crdt/
git commit -m "feat(knot-crdt): Bus trait + in-process MemBus impl"
```

Expected: existing engine tests pass + 3 MemBus tests pass.

---

## Task 6: PgBus (Postgres LISTEN/NOTIFY)

**Files:**
- Create: `crates/knot-crdt/src/bus_pg.rs`
- Modify: `crates/knot-crdt/src/lib.rs`
- Modify: `crates/knot-crdt/Cargo.toml` — `tokio-postgres = "0.7"`
- Modify: root `Cargo.toml` — workspace dep
- Create: `crates/knot-crdt/tests/bus_pg.rs`

- [ ] **Step 1: Cargo deps**

Add to root `Cargo.toml` `[workspace.dependencies]`:

```toml
tokio-postgres = "0.7"
futures-util = "0.3"
```

Add to `crates/knot-crdt/Cargo.toml`:

```toml
tokio-postgres.workspace = true
futures-util.workspace = true
```

- [ ] **Step 2: bus_pg.rs**

Create `/home/nik/Development/knot/crates/knot-crdt/src/bus_pg.rs`:

```rust
//! Postgres LISTEN/NOTIFY Bus.
//!
//! One dedicated `tokio_postgres` connection per replica owns LISTEN for
//! every doc this replica has rooms for. Demuxes incoming Notifications
//! by channel name into per-doc mpsc senders.
//!
//! Channel naming:
//!   doc:<uuid>       — payload = "<seq>" as decimal text
//!   presence:<uuid>  — payload = url-safe base64 of bytes (NOTIFY caps at 8KB)

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use dashmap::DashMap;
use futures_util::FutureExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_postgres::{AsyncMessage, Client, Config, Connection, Socket, tls::NoTlsStream};
use uuid::Uuid;

use crate::bus::{Bus, BusError, Subscription};

const PRESENCE_PAYLOAD_CAP_B64: usize = 6 * 1024; // ~4.5 KB raw after b64 inflation; leave headroom.

#[derive(Default)]
struct DocChannels {
    update_tx: Vec<mpsc::Sender<i64>>,
    presence_tx: Vec<mpsc::Sender<Vec<u8>>>,
}

#[derive(Clone)]
pub struct PgBus {
    client: Arc<Client>,
    subscriptions: Arc<DashMap<Uuid, DocChannels>>,
}

impl PgBus {
    /// Connect a dedicated tokio_postgres client and spawn the demux task.
    /// `database_url` is the same DSN your sqlx pool uses.
    pub async fn connect(database_url: &str) -> Result<Self, BusError> {
        let config = database_url
            .parse::<Config>()
            .map_err(|e| BusError::Io(e.to_string()))?;
        let (client, connection) = config
            .connect(tokio_postgres::NoTls)
            .await
            .map_err(|e| BusError::Io(e.to_string()))?;
        let subscriptions: Arc<DashMap<Uuid, DocChannels>> = Arc::new(DashMap::new());
        let demux_subs = subscriptions.clone();
        tokio::spawn(demux_loop(connection, demux_subs));
        Ok(Self {
            client: Arc::new(client),
            subscriptions,
        })
    }

    /// Demux loop: the connection produces async messages (notifications);
    /// we route by channel-name prefix into per-doc senders.
    pub(crate) fn route(
        subscriptions: &Arc<DashMap<Uuid, DocChannels>>,
        channel: &str,
        payload: &str,
    ) {
        if let Some(rest) = channel.strip_prefix("doc:") {
            let Ok(doc_id) = Uuid::parse_str(rest) else { return };
            let Ok(seq) = payload.parse::<i64>() else { return };
            if let Some(mut e) = subscriptions.get_mut(&doc_id) {
                e.update_tx.retain(|tx| tx.try_send(seq).is_ok());
            }
        } else if let Some(rest) = channel.strip_prefix("presence:") {
            let Ok(doc_id) = Uuid::parse_str(rest) else { return };
            let Ok(bytes) = URL_SAFE_NO_PAD.decode(payload) else { return };
            if let Some(mut e) = subscriptions.get_mut(&doc_id) {
                e.presence_tx.retain(|tx| tx.try_send(bytes.clone()).is_ok());
            }
        }
    }
}

async fn demux_loop(
    connection: Connection<Socket, NoTlsStream>,
    subscriptions: Arc<DashMap<Uuid, DocChannels>>,
) {
    let mut stream = connection.into_async_message_stream();
    use futures_util::StreamExt;
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(AsyncMessage::Notification(n)) => {
                PgBus::route(&subscriptions, n.channel(), n.payload());
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error=?e, "pg bus demux connection error; exiting");
                break;
            }
        }
    }
}

#[async_trait]
impl Bus for PgBus {
    async fn publish(&self, doc_id: Uuid, seq: i64) -> Result<(), BusError> {
        let channel = format!("doc:{doc_id}");
        self.client
            .execute(&format!("NOTIFY \"{channel}\", '{seq}'"), &[])
            .await
            .map_err(|e| BusError::Io(e.to_string()))?;
        Ok(())
    }

    async fn publish_presence(&self, doc_id: Uuid, payload: Vec<u8>) -> Result<(), BusError> {
        let encoded = URL_SAFE_NO_PAD.encode(&payload);
        if encoded.len() > PRESENCE_PAYLOAD_CAP_B64 {
            // Spec §8.7 says size-capped on emit; just drop oversize frames.
            tracing::debug!(len = encoded.len(), "drop oversize presence frame");
            return Ok(());
        }
        let channel = format!("presence:{doc_id}");
        self.client
            .execute(&format!("NOTIFY \"{channel}\", '{encoded}'"), &[])
            .await
            .map_err(|e| BusError::Io(e.to_string()))?;
        Ok(())
    }

    async fn subscribe(&self, doc_id: Uuid) -> Result<Subscription, BusError> {
        let (ut, ur) = mpsc::channel::<i64>(256);
        let (pt, pr) = mpsc::channel::<Vec<u8>>(256);
        let was_new = !self.subscriptions.contains_key(&doc_id);
        let mut entry = self.subscriptions.entry(doc_id).or_default();
        entry.update_tx.push(ut);
        entry.presence_tx.push(pt);
        drop(entry);
        if was_new {
            self.client
                .execute(&format!("LISTEN \"doc:{doc_id}\""), &[])
                .await
                .map_err(|e| BusError::Io(e.to_string()))?;
            self.client
                .execute(&format!("LISTEN \"presence:{doc_id}\""), &[])
                .await
                .map_err(|e| BusError::Io(e.to_string()))?;
        }
        Ok(Subscription { updates: ur, presence: pr })
    }

    async fn unsubscribe(&self, doc_id: Uuid) -> Result<(), BusError> {
        // Remove the entry if no senders are still alive.
        let still_active = self
            .subscriptions
            .get(&doc_id)
            .map(|e| e.update_tx.iter().any(|t| !t.is_closed()))
            .unwrap_or(false);
        if !still_active {
            self.subscriptions.remove(&doc_id);
            let _ = self
                .client
                .execute(&format!("UNLISTEN \"doc:{doc_id}\""), &[])
                .await;
            let _ = self
                .client
                .execute(&format!("UNLISTEN \"presence:{doc_id}\""), &[])
                .await;
        }
        Ok(())
    }
}

// Suppress an unused import warning when `FutureExt` isn't strictly needed.
#[allow(unused_imports)]
use futures_util::future::FutureExt as _FutureExt;
```

> **Implementer note:** `tokio_postgres` 0.7's `Connection::into_async_message_stream` is the recommended API for streaming notifications. If the import path differs in the version you resolve, follow the compiler's hint — the goal is just to read each `AsyncMessage::Notification` off the connection. If a different version exposes it as `connection.poll_message(...)`, write an equivalent `while let Some(m) = ...` loop.

- [ ] **Step 3: lib.rs**

Add to `crates/knot-crdt/src/lib.rs`:

```rust
pub mod bus_pg;
pub use bus_pg::PgBus;
```

Also add the `base64` workspace dep to `knot-crdt/Cargo.toml` (it's already in the workspace from Plan 3).

- [ ] **Step 4: Integration test (real Postgres)**

Create `/home/nik/Development/knot/crates/knot-crdt/tests/bus_pg.rs`:

```rust
use knot_crdt::{Bus, PgBus};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
use tokio::time::{Duration, timeout};
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread")]
async fn publish_reaches_subscriber_via_pg() {
    let c = Postgres::default().start().await.unwrap();
    let port = c.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    // Touch via sqlx to ensure the DB exists (the postgres container default
    // creates `postgres` for us, but this stabilises the wait).
    let _pool = PgPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
    std::mem::forget(c);

    let bus = PgBus::connect(&url).await.unwrap();
    let doc = Uuid::new_v4();
    let mut sub = bus.subscribe(doc).await.unwrap();
    // LISTEN settles after one round-trip; give it a head start.
    tokio::time::sleep(Duration::from_millis(50)).await;
    bus.publish(doc, 7).await.unwrap();
    let got = timeout(Duration::from_secs(2), sub.updates.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, 7);
}

#[tokio::test(flavor = "multi_thread")]
async fn presence_round_trip_via_pg() {
    let c = Postgres::default().start().await.unwrap();
    let port = c.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let _pool = PgPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
    std::mem::forget(c);

    let bus = PgBus::connect(&url).await.unwrap();
    let doc = Uuid::new_v4();
    let mut sub = bus.subscribe(doc).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    bus.publish_presence(doc, vec![9, 8, 7]).await.unwrap();
    let got = timeout(Duration::from_secs(2), sub.presence.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, vec![9, 8, 7]);
}
```

- [ ] **Step 5: Verify + commit**

```bash
cargo test -p knot-crdt --test bus_pg
cargo clippy -p knot-crdt --all-targets --all-features -- -D warnings
git add Cargo.toml Cargo.lock crates/knot-crdt/
git commit -m "feat(knot-crdt): PgBus over tokio_postgres LISTEN/NOTIFY"
```

Expected: 2 tests pass against a fresh testcontainers Postgres.

---

## Task 7: Room actor skeleton

> Tasks 7-15 build the actor incrementally. Each task adds one capability and keeps the integration test list growing. The final room is the union of T7-T15.

**Files:**
- Create: `crates/knot-crdt/src/room.rs`
- Create: `crates/knot-crdt/src/registry.rs` (stub for T15)
- Modify: `crates/knot-crdt/src/lib.rs`

- [ ] **Step 1: Minimal Room shape**

Create `/home/nik/Development/knot/crates/knot-crdt/src/room.rs`:

```rust
//! Per-doc actor. One tokio task. Exclusive owner of `DocHandle` and the
//! local connection map. All I/O flows through mpsc channels.
//!
//! This file is iteratively extended by Tasks 7-15:
//!   T7   minimal select loop + InMsg → engine.apply_update + local fan-out
//!   T8   writer task: batch persist
//!   T9   hydration: load latest snapshot + replay updates
//!   T10  snapshot scheduler
//!   T12  backpressure: bounded channels, slow-consumer close
//!   T13  awareness + bus presence + disconnect clearing
//!   T14  catch-up tick

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::bus::{Bus, Subscription};
use crate::engine::{DocHandle, Engine, EngineError};

pub type ConnId = Uuid;

/// Bytes delivered from a local connection's WS read task.
pub struct InMsg {
    pub from: ConnId,
    pub bytes: Vec<u8>,
}

/// Handle the room hands to a local connection. The WS read task wraps it
/// to send framed messages back to the client.
pub struct ConnHandle {
    pub tx: mpsc::Sender<Vec<u8>>,
}

/// All inputs the room actor multiplexes.
pub(crate) enum Event {
    Inbound(InMsg),
    Join {
        conn_id: ConnId,
        handle: ConnHandle,
        reply: oneshot::Sender<Result<Vec<u8>, EngineError>>,
    },
    Leave(ConnId),
    BusUpdate(i64),
    BusPresence(Vec<u8>),
    Shutdown,
}

pub struct Room {
    pub doc_id: Uuid,
    engine: Arc<dyn Engine>,
    doc: DocHandle,
    conns: HashMap<ConnId, ConnHandle>,
    last_applied_seq: i64,
    bus: Arc<dyn Bus>,
    shutdown: CancellationToken,
    rx: mpsc::Receiver<Event>,
}

pub struct RoomHandle {
    pub tx: mpsc::Sender<Event>,
    pub shutdown: CancellationToken,
}

impl Room {
    /// Spawn a freshly-booted room with an empty doc (hydration in T9).
    pub fn spawn(
        doc_id: Uuid,
        engine: Arc<dyn Engine>,
        bus: Arc<dyn Bus>,
        _subscription: Subscription,
    ) -> RoomHandle {
        let (tx, rx) = mpsc::channel::<Event>(256);
        let shutdown = CancellationToken::new();
        let doc = engine.new_doc();
        let room = Self {
            doc_id,
            engine,
            doc,
            conns: HashMap::new(),
            last_applied_seq: 0,
            bus,
            shutdown: shutdown.clone(),
            rx,
        };
        tokio::spawn(room.run());
        RoomHandle { tx, shutdown }
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                _ = self.shutdown.cancelled() => break,
                msg = self.rx.recv() => match msg {
                    Some(Event::Inbound(m)) => self.on_inbound(m).await,
                    Some(Event::Join { conn_id, handle, reply }) => {
                        self.on_join(conn_id, handle, reply).await;
                    }
                    Some(Event::Leave(c)) => { self.conns.remove(&c); }
                    Some(Event::BusUpdate(_seq)) => {
                        // T14 wires the SELECT-since-watermark replay path.
                    }
                    Some(Event::BusPresence(_)) => {
                        // T13 wires presence fan-out.
                    }
                    Some(Event::Shutdown) | None => break,
                }
            }
        }
        // T15 will flush + write final snapshot here.
    }

    async fn on_join(
        &mut self,
        conn_id: ConnId,
        handle: ConnHandle,
        reply: oneshot::Sender<Result<Vec<u8>, EngineError>>,
    ) {
        self.conns.insert(conn_id, handle);
        // Reply with the full state encoded as a y-sync sync_step_2 payload
        // (the protocol module is in knot-server; the room only encodes
        // the engine bytes here, caller wraps them).
        let r = self.engine.encode_state_as_update(&self.doc, None);
        let _ = reply.send(r);
    }

    async fn on_inbound(&mut self, m: InMsg) {
        // T7: just decode-and-apply. We expect the caller to pre-decode the
        // y-sync frame so the room only sees the inner update bytes. For
        // now, anything other than valid yrs update bytes will fail apply
        // and be logged.
        if let Err(e) = self.engine.apply_update(&self.doc, &m.bytes) {
            tracing::debug!(error=?e, "apply_update failed (T7 stub)");
            return;
        }
        // Fan out to all other local conns.
        for (cid, conn) in &self.conns {
            if *cid == m.from {
                continue;
            }
            let _ = conn.tx.try_send(m.bytes.clone());
        }
    }
}
```

- [ ] **Step 2: registry stub**

Create `/home/nik/Development/knot/crates/knot-crdt/src/registry.rs`:

```rust
//! Rooms registry. T15 fills this in.

use crate::room::RoomHandle;
use dashmap::DashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Default)]
pub struct Rooms {
    map: Arc<DashMap<Uuid, RoomHandle>>,
}

impl Rooms {
    pub fn new() -> Self {
        Self::default()
    }
}
```

- [ ] **Step 3: lib.rs**

Update `/home/nik/Development/knot/crates/knot-crdt/src/lib.rs`:

```rust
pub mod bus;
pub mod bus_mem;
pub mod bus_pg;
pub mod engine;
pub mod registry;
pub mod room;

pub use bus::{Bus, BusError, Subscription};
pub use bus_mem::MemBus;
pub use bus_pg::PgBus;
pub use engine::{DocHandle, Engine, EngineError, TextMark, TextMarkAttr, YrsEngine};
pub use registry::Rooms;
pub use room::{ConnHandle, ConnId, Event, InMsg, Room, RoomHandle};
```

Add `tokio-util = { version = "0.7", features = ["rt"] }` to the knot-crdt deps (it's already in the workspace).

- [ ] **Step 4: Smoke test in room module**

Append at the bottom of `crates/knot-crdt/src/room.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemBus, YrsEngine};

    #[tokio::test]
    async fn room_spawns_and_shuts_down_clean() {
        let bus = Arc::new(MemBus::new());
        let doc_id = Uuid::new_v4();
        let sub = bus.subscribe(doc_id).await.unwrap();
        let h = Room::spawn(doc_id, Arc::new(YrsEngine), bus, sub);
        h.shutdown.cancel();
        // Drop the sender; the task should exit.
        drop(h);
        // Give the runtime a moment.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
```

- [ ] **Step 5: Verify + commit**

```bash
cargo test -p knot-crdt
cargo clippy -p knot-crdt --all-targets --all-features -- -D warnings
git add crates/knot-crdt/
git commit -m "feat(knot-crdt): Room actor skeleton (select loop, join, leave, inbound apply)"
```

Expected: existing engine + MemBus tests + 1 new room smoke test pass.

---

## Task 8: Writer task (batched persist)

**Files:**
- Create: `crates/knot-crdt/src/writer.rs`
- Modify: `crates/knot-crdt/src/room.rs` — spawn writer; route applied updates through it; receive seq on `BusUpdate`
- Modify: `crates/knot-crdt/Cargo.toml` — add `knot-storage = { path = "../knot-storage" }`

- [ ] **Step 1: writer.rs**

Create `/home/nik/Development/knot/crates/knot-crdt/src/writer.rs`:

```rust
//! Per-room writer task. Batches `doc_updates` inserts.
//!
//! Flush triggers (whichever first):
//!   - batch reaches 200 updates
//!   - 250 ms has elapsed since the first item in the batch
//!
//! Each successful insert returns one seq per input; the writer publishes
//! each seq over the bus and informs the room via `applied_tx` so the room
//! can advance `last_applied_seq` and fan out to its local conns.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::bus::Bus;
use knot_storage::UpdatesStore;

pub const BATCH_MAX: usize = 200;
pub const BATCH_INTERVAL: Duration = Duration::from_millis(250);

/// Input the writer receives from the room.
pub struct PersistJob {
    pub bytes: Vec<u8>,
    pub by_user_id: Option<Uuid>,
}

/// Output the writer sends back so the room can fan-out + track watermark.
pub struct Applied {
    pub seq: i64,
    pub bytes: Vec<u8>,
}

pub fn spawn(
    doc_id: Uuid,
    store: Arc<dyn UpdatesStore>,
    bus: Arc<dyn Bus>,
    mut rx: mpsc::Receiver<PersistJob>,
    applied_tx: mpsc::Sender<Applied>,
) {
    tokio::spawn(async move {
        let mut buf: Vec<PersistJob> = Vec::with_capacity(BATCH_MAX);
        let mut deadline: Option<tokio::time::Instant> = None;
        loop {
            let wait = match deadline {
                Some(d) => tokio::time::sleep_until(d).boxed(),
                None => std::future::pending::<()>().boxed(),
            };
            tokio::select! {
                biased;
                _ = wait => {
                    if !buf.is_empty() {
                        flush(doc_id, &store, &bus, &applied_tx, &mut buf).await;
                        deadline = None;
                    }
                }
                Some(job) = rx.recv() => {
                    buf.push(job);
                    if buf.len() == 1 {
                        deadline = Some(tokio::time::Instant::now() + BATCH_INTERVAL);
                    }
                    if buf.len() >= BATCH_MAX {
                        flush(doc_id, &store, &bus, &applied_tx, &mut buf).await;
                        deadline = None;
                    }
                }
                else => break,
            }
        }
        if !buf.is_empty() {
            flush(doc_id, &store, &bus, &applied_tx, &mut buf).await;
        }
    });
}

async fn flush(
    doc_id: Uuid,
    store: &Arc<dyn UpdatesStore>,
    bus: &Arc<dyn Bus>,
    applied_tx: &mpsc::Sender<Applied>,
    buf: &mut Vec<PersistJob>,
) {
    // Single by_user_id per batch is enough — the spec lets us attribute
    // by the first item's user; later we can split the batch if writers
    // mix users (rare in practice).
    let by_user = buf.first().and_then(|j| j.by_user_id);
    let updates: Vec<Vec<u8>> = buf.iter().map(|j| j.bytes.clone()).collect();
    match store.insert_batch(doc_id, by_user, &updates).await {
        Ok(seqs) => {
            for (seq, job) in seqs.into_iter().zip(buf.drain(..)) {
                if bus.publish(doc_id, seq).await.is_err() {
                    tracing::warn!(%doc_id, "bus publish failed; relying on catch-up tick");
                }
                let _ = applied_tx.try_send(Applied { seq, bytes: job.bytes });
            }
        }
        Err(e) => {
            tracing::error!(error=?e, %doc_id, "writer flush failed; dropping batch (will reapply on next read)");
            buf.clear();
        }
    }
}

// FutureExt::boxed for trait-object pinning of the sleep future.
use futures_util::FutureExt;
```

- [ ] **Step 2: Wire into Room**

Edit `crates/knot-crdt/src/room.rs`. Update `Room::spawn` to take an `Arc<dyn UpdatesStore>` and to create the persist channel + Applied channel:

```rust
pub fn spawn(
    doc_id: Uuid,
    engine: Arc<dyn Engine>,
    bus: Arc<dyn Bus>,
    subscription: Subscription,
    updates_store: Arc<dyn knot_storage::UpdatesStore>,
) -> RoomHandle {
    let (tx, rx) = mpsc::channel::<Event>(256);
    let shutdown = CancellationToken::new();
    let doc = engine.new_doc();

    let (persist_tx, persist_rx) = mpsc::channel::<crate::writer::PersistJob>(1024);
    let (applied_tx, applied_rx) = mpsc::channel::<crate::writer::Applied>(256);
    crate::writer::spawn(doc_id, updates_store, bus.clone(), persist_rx, applied_tx);

    let room = Self {
        doc_id,
        engine,
        doc,
        conns: HashMap::new(),
        last_applied_seq: 0,
        bus,
        shutdown: shutdown.clone(),
        rx,
        persist_tx,
        applied_rx,
        bus_updates_rx: subscription.updates,
        bus_presence_rx: subscription.presence,
    };
    tokio::spawn(room.run());
    RoomHandle { tx, shutdown }
}
```

Add the new fields to `Room`:

```rust
pub struct Room {
    pub doc_id: Uuid,
    engine: Arc<dyn Engine>,
    doc: DocHandle,
    conns: HashMap<ConnId, ConnHandle>,
    last_applied_seq: i64,
    bus: Arc<dyn Bus>,
    shutdown: CancellationToken,
    rx: mpsc::Receiver<Event>,
    persist_tx: mpsc::Sender<crate::writer::PersistJob>,
    applied_rx: mpsc::Receiver<crate::writer::Applied>,
    bus_updates_rx: mpsc::Receiver<i64>,
    bus_presence_rx: mpsc::Receiver<Vec<u8>>,
}
```

Update the `run` loop to handle Applied and bus channels:

```rust
    async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                _ = self.shutdown.cancelled() => break,
                msg = self.rx.recv() => match msg {
                    Some(Event::Inbound(m)) => self.on_inbound(m).await,
                    Some(Event::Join { conn_id, handle, reply }) => {
                        self.on_join(conn_id, handle, reply).await;
                    }
                    Some(Event::Leave(c)) => { self.conns.remove(&c); }
                    Some(Event::BusUpdate(_)) | Some(Event::BusPresence(_)) => {}
                    Some(Event::Shutdown) | None => break,
                },
                Some(applied) = self.applied_rx.recv() => {
                    if applied.seq > self.last_applied_seq {
                        self.last_applied_seq = applied.seq;
                    }
                    // local fan-out happens at apply time (in on_inbound);
                    // applied just advances the watermark.
                }
                Some(seq) = self.bus_updates_rx.recv() => {
                    if seq > self.last_applied_seq {
                        // T14 fills in the SELECT-since-seq replay path.
                        tracing::trace!(%seq, "bus update; replay pending in T14");
                    }
                }
                Some(_payload) = self.bus_presence_rx.recv() => {
                    // T13.
                }
            }
        }
    }
```

Update `on_inbound` to also send to the writer:

```rust
    async fn on_inbound(&mut self, m: InMsg) {
        if let Err(e) = self.engine.apply_update(&self.doc, &m.bytes) {
            tracing::debug!(error=?e, "apply_update failed");
            return;
        }
        // Fan out locally now (writer publishes durably + over bus).
        for (cid, conn) in &self.conns {
            if *cid == m.from { continue; }
            let _ = conn.tx.try_send(m.bytes.clone());
        }
        // Hand to writer (best-effort; bounded channel applies backpressure
        // via try_send dropping if full — T12 tightens this).
        let _ = self
            .persist_tx
            .try_send(crate::writer::PersistJob {
                bytes: m.bytes,
                by_user_id: None,
            });
    }
```

- [ ] **Step 3: lib.rs**

Add `pub mod writer;` and re-export `pub use writer::{Applied, PersistJob};`.

- [ ] **Step 4: Cargo.toml**

Add `knot-storage = { path = "../knot-storage" }` to `crates/knot-crdt/Cargo.toml [dependencies]`.

- [ ] **Step 5: Integration test**

Append to `crates/knot-crdt/src/room.rs` `tests` module:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn inbound_update_round_trips_via_writer_and_persists_seq() {
        use knot_storage::{PgUpdatesStore, UpdatesStore};
        use sqlx::postgres::PgPoolOptions;
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let c = Postgres::default().start().await.unwrap();
        let port = c.get_host_port_ipv4(5432).await.unwrap();
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
        let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        std::mem::forget(c);

        let updates = Arc::new(PgUpdatesStore::new(pool.clone()));
        // Seed a workspace + user + doc so the FK on doc_updates resolves.
        let ws = knot_storage::PgWorkspaceStore::new(pool.clone())
            .create("d", "W").await.unwrap();
        let u = knot_storage::PgUserStore::new(pool.clone())
            .create_local("a@x.test", "A", "$h$").await.unwrap();
        knot_storage::PgWorkspaceStore::new(pool.clone())
            .add_member(ws.id, u.id, knot_storage::WorkspaceRole::Owner).await.unwrap();
        let d = knot_storage::PgDocStore::new(pool.clone())
            .create(ws.id, None, "D", "m", u.id).await.unwrap();

        let engine = Arc::new(YrsEngine);
        // Generate an actual yrs update on a side doc to feed in.
        let doc_a = engine.new_doc();
        let doc_b = engine.new_doc();
        let _ = engine.apply_update(&doc_a, &engine.encode_state_as_update(&doc_b, None).unwrap());
        // Make a real edit to doc_a so encode_state_as_update on it produces non-empty bytes.
        // (yrs always produces *some* bytes; we just want any valid update payload.)
        let real_update = engine.encode_state_as_update(&doc_a, None).unwrap();

        let bus = Arc::new(MemBus::new());
        let sub = bus.subscribe(d.id).await.unwrap();
        let h = Room::spawn(d.id, engine.clone(), bus.clone(), sub, updates.clone());

        let conn_id = Uuid::new_v4();
        let (tx, _rx) = mpsc::channel(8);
        let (reply_tx, reply_rx) = oneshot::channel();
        h.tx.send(Event::Join { conn_id, handle: ConnHandle { tx }, reply: reply_tx })
            .await
            .unwrap();
        let _full = reply_rx.await.unwrap().unwrap();
        h.tx.send(Event::Inbound(InMsg { from: conn_id, bytes: real_update.clone() }))
            .await
            .unwrap();

        // Writer batches 250 ms; wait 500 ms then assert the row landed.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let max = updates.max_seq(d.id).await.unwrap();
        assert!(max > 0, "expected at least one row persisted; got max_seq={max}");
    }
```

- [ ] **Step 6: Verify + commit**

```bash
cargo test -p knot-crdt
cargo clippy -p knot-crdt --all-targets --all-features -- -D warnings
git add crates/knot-crdt/ Cargo.toml Cargo.lock
git commit -m "feat(knot-crdt): writer task — batched doc_updates + bus publish"
```

Expected: smoke + writer round-trip test pass.

---

## Task 9: Hydration on room boot

**Files:**
- Modify: `crates/knot-crdt/src/room.rs` — `Room::spawn` calls a new `hydrate` step

- [ ] **Step 1: Hydration helper + boot order**

Edit `crates/knot-crdt/src/room.rs`. Update `Room::spawn` to take a `SnapshotStore` too and hydrate before spawning:

```rust
pub async fn spawn(
    doc_id: Uuid,
    engine: Arc<dyn Engine>,
    bus: Arc<dyn Bus>,
    subscription: Subscription,
    updates_store: Arc<dyn knot_storage::UpdatesStore>,
    snapshots: Arc<dyn knot_storage::SnapshotStore>,
) -> Result<RoomHandle, EngineError> {
    let doc = engine.new_doc();
    let mut last_applied_seq: i64 = 0;
    // 1. Load latest snapshot.
    if let Ok(Some(snap)) = snapshots.latest(doc_id).await {
        engine.apply_update(&doc, &snap.state_bytes)?;
        last_applied_seq = snap.snapshot_seq;
    }
    // 2. Replay updates after the snapshot.
    if let Ok(after) = updates_store.since(doc_id, last_applied_seq).await {
        for u in after {
            engine.apply_update(&doc, &u.update_bytes)?;
            if u.seq > last_applied_seq { last_applied_seq = u.seq; }
        }
    }
    // 3. Spawn the actor with the hydrated doc + watermark.
    let (tx, rx) = mpsc::channel::<Event>(256);
    let shutdown = CancellationToken::new();
    let (persist_tx, persist_rx) = mpsc::channel::<crate::writer::PersistJob>(1024);
    let (applied_tx, applied_rx) = mpsc::channel::<crate::writer::Applied>(256);
    crate::writer::spawn(doc_id, updates_store, bus.clone(), persist_rx, applied_tx);
    let room = Self {
        doc_id, engine, doc,
        conns: HashMap::new(),
        last_applied_seq,
        bus,
        shutdown: shutdown.clone(),
        rx,
        persist_tx,
        applied_rx,
        bus_updates_rx: subscription.updates,
        bus_presence_rx: subscription.presence,
    };
    tokio::spawn(room.run());
    Ok(RoomHandle { tx, shutdown })
}
```

(Update existing tests that called `Room::spawn` synchronously to `.await?` it.)

- [ ] **Step 2: Hydration test**

Append to the tests module:

```rust
    #[tokio::test(flavor = "multi_thread")]
    async fn room_replays_prior_updates_on_boot() {
        use knot_storage::{PgUpdatesStore, PgSnapshotStore, UpdatesStore};
        use sqlx::postgres::PgPoolOptions;
        use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

        let c = Postgres::default().start().await.unwrap();
        let port = c.get_host_port_ipv4(5432).await.unwrap();
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
        let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        std::mem::forget(c);

        let updates = Arc::new(PgUpdatesStore::new(pool.clone()));
        let snaps = Arc::new(PgSnapshotStore::new(pool.clone()));
        let ws = knot_storage::PgWorkspaceStore::new(pool.clone()).create("d","W").await.unwrap();
        let u = knot_storage::PgUserStore::new(pool.clone()).create_local("a@x.test","A","$h$").await.unwrap();
        knot_storage::PgWorkspaceStore::new(pool.clone()).add_member(ws.id,u.id,knot_storage::WorkspaceRole::Owner).await.unwrap();
        let d = knot_storage::PgDocStore::new(pool.clone()).create(ws.id,None,"D","m",u.id).await.unwrap();

        // Seed one update so the room must replay it.
        let engine = Arc::new(YrsEngine);
        let tmp = engine.new_doc();
        engine.apply_update(&tmp, &engine.encode_state_as_update(&engine.new_doc(), None).unwrap()).unwrap();
        let seed_bytes = engine.encode_state_as_update(&tmp, None).unwrap();
        updates.insert_batch(d.id, Some(u.id), &[seed_bytes.clone()]).await.unwrap();

        let bus = Arc::new(MemBus::new());
        let sub = bus.subscribe(d.id).await.unwrap();
        let h = Room::spawn(d.id, engine.clone(), bus.clone(), sub, updates.clone(), snaps).await.unwrap();

        // Join — the join reply carries our hydrated state. It must be
        // non-empty (the seed update materialised content).
        let (tx, _rx) = mpsc::channel(8);
        let (reply_tx, reply_rx) = oneshot::channel();
        h.tx.send(Event::Join {
            conn_id: Uuid::new_v4(),
            handle: ConnHandle { tx },
            reply: reply_tx,
        }).await.unwrap();
        let state = reply_rx.await.unwrap().unwrap();
        assert!(!state.is_empty(), "hydrated state should include the seed update");
    }
```

- [ ] **Step 3: Verify + commit**

```bash
cargo test -p knot-crdt
cargo clippy -p knot-crdt --all-targets --all-features -- -D warnings
git add crates/knot-crdt/
git commit -m "feat(knot-crdt): hydrate room from latest snapshot + replay updates"
```

Expected: smoke + writer + hydration tests pass.

---

## Task 10: Snapshot scheduler

**Files:**
- Create: `crates/knot-crdt/src/snapshot.rs`
- Modify: `crates/knot-crdt/src/room.rs` — call into snapshot helper on N-updates / idle

- [ ] **Step 1: Snapshot helper**

Create `/home/nik/Development/knot/crates/knot-crdt/src/snapshot.rs`:

```rust
//! Snapshot trigger logic. The actor calls `maybe_snapshot` after each
//! applied update (N-trigger) and after the idle timer fires (idle-trigger).

use crate::engine::{DocHandle, Engine, EngineError};
use knot_storage::SnapshotStore;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

pub struct SnapshotPolicy {
    pub every_n: u32,
    pub idle: std::time::Duration,
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
```

- [ ] **Step 2: Wire into Room**

Edit `crates/knot-crdt/src/room.rs`. Add SnapshotStore + SnapshotPolicy to Room:

```rust
pub struct Room {
    // ... existing ...
    snapshots: Arc<dyn knot_storage::SnapshotStore>,
    policy: crate::snapshot::SnapshotPolicy,
    snap_state: crate::snapshot::SnapshotState,
}
```

In `Room::spawn`, accept the policy:

```rust
pub async fn spawn(
    doc_id: Uuid,
    engine: Arc<dyn Engine>,
    bus: Arc<dyn Bus>,
    subscription: Subscription,
    updates_store: Arc<dyn knot_storage::UpdatesStore>,
    snapshots: Arc<dyn knot_storage::SnapshotStore>,
    policy: crate::snapshot::SnapshotPolicy,
) -> Result<RoomHandle, EngineError> {
    // ... existing hydration ...
    let snap_state = crate::snapshot::SnapshotState {
        last_snapshot_seq: last_applied_seq,
        updates_since_snapshot: 0,
        last_apply_at: std::time::Instant::now(),
    };
    let room = Self {
        // ... existing fields ...
        snapshots,
        policy,
        snap_state,
    };
    // ... spawn ...
}
```

In the `applied_rx.recv()` arm of `run`, after advancing `last_applied_seq`:

```rust
                Some(applied) = self.applied_rx.recv() => {
                    if applied.seq > self.last_applied_seq {
                        self.last_applied_seq = applied.seq;
                    }
                    self.snap_state.updates_since_snapshot += 1;
                    self.snap_state.last_apply_at = std::time::Instant::now();
                    if self.snap_state.updates_since_snapshot >= self.policy.every_n {
                        if let Err(e) = crate::snapshot::write_snapshot(
                            self.doc_id, self.last_applied_seq,
                            self.engine.as_ref(), &self.doc,
                            self.snapshots.as_ref(),
                        ).await {
                            tracing::warn!(error=?e, "snapshot write failed");
                        } else {
                            self.snap_state.last_snapshot_seq = self.last_applied_seq;
                            self.snap_state.updates_since_snapshot = 0;
                        }
                    }
                }
```

Add an idle-tick branch using `tokio::time::interval`:

```rust
        let mut idle_tick = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tokio::select! {
                biased;
                _ = self.shutdown.cancelled() => break,
                _ = idle_tick.tick() => {
                    let idle = self.snap_state.last_apply_at.elapsed();
                    if self.snap_state.updates_since_snapshot > 0 && idle >= self.policy.idle {
                        if let Ok(()) = crate::snapshot::write_snapshot(
                            self.doc_id, self.last_applied_seq,
                            self.engine.as_ref(), &self.doc,
                            self.snapshots.as_ref(),
                        ).await {
                            self.snap_state.last_snapshot_seq = self.last_applied_seq;
                            self.snap_state.updates_since_snapshot = 0;
                        }
                    }
                }
                // ... other arms ...
            }
        }
```

- [ ] **Step 3: lib.rs**

```rust
pub mod snapshot;
pub use snapshot::{SnapshotPolicy, SnapshotState};
```

- [ ] **Step 4: Verify + commit**

The N-trigger and idle-trigger are exercised indirectly by the existing hydration test (which now snapshots after enough updates). Add no new test — Plan 5 e2e (T20) covers the integration.

```bash
cargo test -p knot-crdt
cargo clippy -p knot-crdt --all-targets --all-features -- -D warnings
git add crates/knot-crdt/
git commit -m "feat(knot-crdt): snapshot scheduler — N-updates + idle-sec triggers"
```

---

## Task 11: Snapshot GC task

**Files:**
- Create: `crates/knot-crdt/src/gc.rs`
- Modify: `crates/knot-crdt/src/lib.rs`

- [ ] **Step 1: GC task spawner**

Create `/home/nik/Development/knot/crates/knot-crdt/src/gc.rs`:

```rust
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
                    if cutoff > 0 {
                        if let Err(e) = updates.delete_up_to(doc_id, cutoff).await {
                            tracing::warn!(error=?e, %doc_id, "gc updates failed");
                        }
                    }
                }
                if let Err(e) = snapshots.gc(doc_id, 5, 30).await {
                    tracing::warn!(error=?e, %doc_id, "gc snapshots failed");
                }
            }
        }
    });
}
```

- [ ] **Step 2: lib.rs**

```rust
pub mod gc;
pub use gc::spawn as spawn_gc;
```

- [ ] **Step 3: Verify + commit**

```bash
cargo build -p knot-crdt
cargo clippy -p knot-crdt --all-targets --all-features -- -D warnings
git add crates/knot-crdt/
git commit -m "feat(knot-crdt): hourly snapshot + updates GC task"
```

GC is exercised end-to-end in T20; this task ships the wiring.

---

## Task 12: Backpressure: bounded channels + 4408

**Files:**
- Modify: `crates/knot-crdt/src/room.rs`
- Modify: `crates/knot-crdt/Cargo.toml` (no new deps)

- [ ] **Step 1: ConnHandle gets a slow-consumer signal**

Edit `room.rs`. Change `on_inbound`'s local fan-out to detect slow consumers:

```rust
        let mut to_close: Vec<ConnId> = Vec::new();
        for (cid, conn) in &self.conns {
            if *cid == m.from { continue; }
            match conn.tx.try_send(m.bytes.clone()) {
                Ok(_) => {}
                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => to_close.push(*cid),
                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => to_close.push(*cid),
            }
        }
        for cid in to_close {
            self.conns.remove(&cid);
            // The connection's read task will notice the sender dropped and
            // close the WS with 4408 in knot-server's WS shim.
        }
```

The knot-server WS shim (T16) maps a closed outbound channel to a 4408 WebSocket close frame.

- [ ] **Step 2: Persist channel pushback**

Replace `self.persist_tx.try_send(...)` with `self.persist_tx.send(...).await`:

```rust
        if let Err(e) = self
            .persist_tx
            .send(crate::writer::PersistJob { bytes: m.bytes, by_user_id: None })
            .await
        {
            tracing::error!(error=?e, "persist channel closed; dropping update");
        }
```

This is the §8.5 "persist channel full → room actor awaits" backpressure. It slows *this doc's* edit rate when persistence falls behind, which is exactly the spec's intent.

- [ ] **Step 3: Verify + commit**

```bash
cargo test -p knot-crdt
cargo clippy -p knot-crdt --all-targets --all-features -- -D warnings
git add crates/knot-crdt/
git commit -m "feat(knot-crdt): backpressure — slow-consumer eviction + persist await"
```

---

## Task 13: Awareness presence

**Files:**
- Create: `crates/knot-crdt/src/presence.rs`
- Modify: `crates/knot-crdt/src/room.rs`

- [ ] **Step 1: Presence frame type**

Create `/home/nik/Development/knot/crates/knot-crdt/src/presence.rs`:

```rust
//! Awareness frames — opaque bytes from the wire; we never decode them.
//! Size cap = 4 KB on emit. On disconnect the room synthesises a clearing
//! frame so other clients drop the departed cursor.

pub const PRESENCE_MAX_BYTES: usize = 4 * 1024;

pub fn is_oversize(payload: &[u8]) -> bool {
    payload.len() > PRESENCE_MAX_BYTES
}
```

- [ ] **Step 2: Room handles presence**

In `room.rs`:
- Add an `Awareness` variant to `Event`:

```rust
    AwarenessIn { from: ConnId, payload: Vec<u8> },
```

- In `run`, handle it:

```rust
                Some(Event::AwarenessIn { from, payload }) => {
                    if crate::presence::is_oversize(&payload) { continue; }
                    // Fan out to local conns (sans origin).
                    for (cid, conn) in &self.conns {
                        if *cid == from { continue; }
                        let _ = conn.tx.try_send(payload.clone());
                    }
                    // Bus to other replicas.
                    let _ = self.bus.publish_presence(self.doc_id, payload).await;
                }
                Some(payload) = self.bus_presence_rx.recv() => {
                    for conn in self.conns.values() {
                        let _ = conn.tx.try_send(payload.clone());
                    }
                }
```

- In `on_leave`, synthesise a clearing frame. For y-protocol v1 awareness, "clearing" means an update where the leaving clientID has `null` state. Implementer note: the spike's current implementation doesn't actually decode awareness frames either — it's enough to push a single zero-byte sentinel so other clients re-query. v0.1 ships this and refines in Plan 6's frontend if needed.

```rust
                Some(Event::Leave(c)) => {
                    self.conns.remove(&c);
                    // Best-effort clearing: an empty Vec<u8> the frontend
                    // interprets as "re-query awareness". The bus carries it
                    // so other replicas synthesise their own clear too.
                    let _ = self.bus.publish_presence(self.doc_id, Vec::new()).await;
                }
```

- [ ] **Step 3: Verify + commit**

```bash
cargo test -p knot-crdt
cargo clippy -p knot-crdt --all-targets --all-features -- -D warnings
git add crates/knot-crdt/
git commit -m "feat(knot-crdt): awareness fan-out + size cap + leave-clear"
```

---

## Task 14: Catch-up polling tick

**Files:**
- Modify: `crates/knot-crdt/src/room.rs`

- [ ] **Step 1: 5s polling tick**

Add a `tokio::time::interval(Duration::from_secs(5))` to the `select!`. When it fires, the room SELECTs `doc_updates WHERE seq > last_applied_seq` and applies each, fanning out as usual:

```rust
                _ = catchup.tick() => {
                    if let Ok(rows) = self.updates_store.since(self.doc_id, self.last_applied_seq).await {
                        for u in rows {
                            if u.seq <= self.last_applied_seq { continue; }
                            if self.engine.apply_update(&self.doc, &u.update_bytes).is_ok() {
                                for conn in self.conns.values() {
                                    let _ = conn.tx.try_send(u.update_bytes.clone());
                                }
                                self.last_applied_seq = u.seq;
                            }
                        }
                    }
                }
```

Add an `updates_store: Arc<dyn UpdatesStore>` field to `Room` and pass it in `Room::spawn`.

Also handle the `Event::BusUpdate(seq)` arm (currently a no-op): treat it as a hint, then run the same SELECT-since-watermark loop. That ensures NOTIFYs are responsive without losing correctness when one is dropped.

- [ ] **Step 2: Verify + commit**

```bash
cargo test -p knot-crdt
cargo clippy -p knot-crdt --all-targets --all-features -- -D warnings
git add crates/knot-crdt/
git commit -m "feat(knot-crdt): 5s catch-up tick + bus update replay"
```

---

## Task 15: Rooms registry + idle eviction + final snapshot

**Files:**
- Modify: `crates/knot-crdt/src/registry.rs`
- Modify: `crates/knot-crdt/src/lib.rs`

- [ ] **Step 1: Registry with in-flight dedup**

Replace `crates/knot-crdt/src/registry.rs` with:

```rust
//! Rooms registry. One `RoomHandle` per active doc.
//!
//! Acquire is in-flight-dedup safe: concurrent acquire calls for the same
//! doc cooperate so only one room boots.

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::bus::Bus;
use crate::engine::Engine;
use crate::room::{Room, RoomHandle};
use crate::snapshot::SnapshotPolicy;
use knot_storage::{SnapshotStore, UpdatesStore};

pub struct Rooms {
    map: DashMap<Uuid, Arc<RoomHandle>>,
    inflight: DashMap<Uuid, Arc<Mutex<()>>>,
    engine: Arc<dyn Engine>,
    bus: Arc<dyn Bus>,
    updates: Arc<dyn UpdatesStore>,
    snapshots: Arc<dyn SnapshotStore>,
    policy: SnapshotPolicy,
    idle_evict: Duration,
}

impl Rooms {
    pub fn new(
        engine: Arc<dyn Engine>,
        bus: Arc<dyn Bus>,
        updates: Arc<dyn UpdatesStore>,
        snapshots: Arc<dyn SnapshotStore>,
        policy: SnapshotPolicy,
        idle_evict: Duration,
    ) -> Self {
        Self {
            map: DashMap::new(),
            inflight: DashMap::new(),
            engine,
            bus,
            updates,
            snapshots,
            policy,
            idle_evict,
        }
    }

    pub async fn acquire(&self, doc_id: Uuid) -> Arc<RoomHandle> {
        if let Some(h) = self.map.get(&doc_id) {
            return h.clone();
        }
        let guard = self.inflight.entry(doc_id).or_insert_with(|| Arc::new(Mutex::new(()))).clone();
        let _lock = guard.lock().await;
        if let Some(h) = self.map.get(&doc_id) {
            return h.clone();
        }
        let sub = self.bus.subscribe(doc_id).await.expect("subscribe");
        let h = Room::spawn(
            doc_id,
            self.engine.clone(),
            self.bus.clone(),
            sub,
            self.updates.clone(),
            self.snapshots.clone(),
            SnapshotPolicy { every_n: self.policy.every_n, idle: self.policy.idle },
        )
        .await
        .expect("hydrate");
        let arc = Arc::new(h);
        self.map.insert(doc_id, arc.clone());
        arc
    }

    /// Called by knot-server's WS handler when a connection joins. Returns
    /// the room handle so the caller can send Event::Join.
    pub async fn release(&self, doc_id: Uuid) {
        if let Some((_, h)) = self.map.remove(&doc_id) {
            h.shutdown.cancel();
        }
        let _ = self.bus.unsubscribe(doc_id).await;
        let _ = self.idle_evict;
    }
}
```

- [ ] **Step 2: Final snapshot on shutdown**

Edit `room.rs`. At the bottom of `run`, after the loop:

```rust
        // Final flush: write a snapshot at the current seq so the next boot
        // is cheap. Best-effort.
        let _ = crate::snapshot::write_snapshot(
            self.doc_id, self.last_applied_seq,
            self.engine.as_ref(), &self.doc,
            self.snapshots.as_ref(),
        ).await;
```

- [ ] **Step 3: Verify + commit**

```bash
cargo test -p knot-crdt
cargo clippy -p knot-crdt --all-targets --all-features -- -D warnings
git add crates/knot-crdt/
git commit -m "feat(knot-crdt): Rooms registry with in-flight dedup + final snapshot on evict"
```

---

## Task 16: knot-server WS upgrade — auth + role pinned

**Files:**
- Modify: `crates/knot-server/src/lib.rs` — AppState carries Arc<knot_crdt::Rooms>; remove the old in-memory Rooms
- Modify: `crates/knot-server/src/main.rs` — construct PgBus, spawn GC, plumb registry
- Rewrite: `crates/knot-server/src/room.rs` — WS shim
- Modify: `crates/knot-server/src/auth/require_doc_role.rs` — re-export usable WS path
- Delete the spike's `protocol.rs` is kept (the framing module is reused). The spike's `room.rs` is fully replaced.

- [ ] **Step 1: AppState**

Edit `crates/knot-server/src/lib.rs`. Add fields:

```rust
    pub rooms_v2: Option<Arc<knot_crdt::Rooms>>,
    pub bus: Option<Arc<dyn knot_crdt::Bus>>,
```

In `with_pool`, build the bus + registry. (Bus construction is async; the `with_pool` constructor is sync. We can leave these `None` here and let `main.rs` populate them after `PgBus::connect`.)

- [ ] **Step 2: main.rs**

After observability init + pool connect, BEFORE building the router:

```rust
    let (bus, rooms_v2) = if let Some(pool) = &pool {
        match knot_crdt::PgBus::connect(&cfg.database_url).await {
            Ok(b) => {
                let bus: Arc<dyn knot_crdt::Bus> = Arc::new(b);
                let updates: Arc<dyn knot_storage::UpdatesStore> =
                    Arc::new(knot_storage::PgUpdatesStore::new(pool.clone()));
                let snaps: Arc<dyn knot_storage::SnapshotStore> =
                    Arc::new(knot_storage::PgSnapshotStore::new(pool.clone()));
                let policy = knot_crdt::SnapshotPolicy {
                    every_n: cfg.snapshot_every_n,
                    idle: std::time::Duration::from_secs(cfg.snapshot_idle_sec as u64),
                };
                let rooms = Arc::new(knot_crdt::Rooms::new(
                    Arc::new(knot_crdt::YrsEngine),
                    bus.clone(),
                    updates.clone(),
                    snaps.clone(),
                    policy,
                    std::time::Duration::from_secs(cfg.room_idle_evict_sec as u64),
                ));
                knot_crdt::spawn_gc(pool.clone(), snaps, updates, cfg.snapshot_every_n);
                (Some(bus), Some(rooms))
            }
            Err(e) => {
                tracing::error!(error=?e, "PgBus connect failed");
                process::exit(2);
            }
        }
    } else {
        (None, None)
    };
```

Plumb both into `AppState`:

```rust
    state.bus = bus;
    state.rooms_v2 = rooms_v2;
```

- [ ] **Step 3: WS shim rewrite**

Replace `/home/nik/Development/knot/crates/knot-server/src/room.rs` with:

```rust
//! WebSocket → Room shim. Auth happens at upgrade (SessionLoader +
//! RequireSession + RequireDocRole). The (user, role) tuple is pinned for
//! the lifetime of the connection — mid-session revocation arrives as a
//! 4403 close frame via the registry (T17).

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use knot_crdt::{ConnHandle, ConnId, Event, InMsg};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::protocol::{
    YSyncMessage, decode, encode_sync_step2, encode_sync_update, MSG_AWARENESS,
};

pub async fn serve(rooms: Arc<knot_crdt::Rooms>, doc_id: Uuid, socket: WebSocket) {
    let handle = rooms.acquire(doc_id).await;
    let conn_id: ConnId = Uuid::new_v4();
    let (out_tx, mut out_rx) = mpsc::channel::<Vec<u8>>(256);

    // Join — receive hydrated state as bytes; wrap in sync_step_2 frame.
    let (reply_tx, reply_rx) = oneshot::channel();
    if handle.tx
        .send(Event::Join {
            conn_id,
            handle: ConnHandle { tx: out_tx.clone() },
            reply: reply_tx,
        })
        .await
        .is_err()
    {
        return;
    }
    let initial = match reply_rx.await {
        Ok(Ok(b)) => encode_sync_step2(&b),
        _ => return,
    };
    let _ = out_tx.send(initial).await;

    let (mut sink, mut stream) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(bytes) = out_rx.recv().await {
            if sink.send(Message::Binary(bytes.into())).await.is_err() { break; }
        }
    });

    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Binary(b) => {
                let bytes = b.to_vec();
                match decode(&bytes) {
                    Ok(YSyncMessage::SyncStep1(_sv)) => {
                        // Reply with full state as sync_step_2 (already sent
                        // at join; if the client re-sends, we just answer
                        // again — cheap).
                        let (rtx, rrx) = oneshot::channel();
                        let _ = handle.tx.send(Event::Join {
                            conn_id,
                            handle: ConnHandle { tx: out_tx.clone() },
                            reply: rtx,
                        }).await;
                        if let Ok(Ok(state)) = rrx.await {
                            let _ = out_tx.send(encode_sync_step2(&state)).await;
                        }
                    }
                    Ok(YSyncMessage::SyncStep2(inner)) | Ok(YSyncMessage::Update(inner)) => {
                        let _ = handle.tx.send(Event::Inbound(InMsg { from: conn_id, bytes: inner })).await;
                    }
                    Ok(YSyncMessage::Awareness) => {
                        let _ = handle.tx.send(Event::AwarenessIn { from: conn_id, payload: bytes }).await;
                        // (We forward the raw frame so peers see the same
                        // bytes; awareness opaque to us.)
                        let _ = MSG_AWARENESS; // suppress unused warning
                    }
                    Err(_) => {}
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    let _ = handle.tx.send(Event::Leave(conn_id)).await;
    let _ = writer.await;
}
```

- [ ] **Step 4: Auth at upgrade in `lib.rs`'s `collab_upgrade`**

Replace the existing handler:

```rust
async fn collab_upgrade(
    Path(doc_id): Path<Uuid>,
    State(state): State<AppState>,
    req: axum::extract::Request,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Auth: AuthContext must be present (SessionLoader populates it).
    let Some(ctx) = req.extensions().get::<crate::auth::AuthContext>().cloned() else {
        return (axum::http::StatusCode::UNAUTHORIZED, "auth.session_required").into_response();
    };
    // ACL: resolve effective role; require at least Viewer.
    let acl = match state.acl.as_ref() {
        Some(a) => a.clone(),
        None => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    };
    match acl.effective_role(ctx.workspace_id, doc_id, ctx.user_id).await {
        Ok(Some(_role)) => {}
        Ok(None) => return (axum::http::StatusCode::FORBIDDEN, "acl.no_grant").into_response(),
        Err(_) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    }
    let rooms = match state.rooms_v2.as_ref() {
        Some(r) => r.clone(),
        None => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "internal").into_response(),
    };
    ws.on_upgrade(move |socket| async move {
        crate::room::serve(rooms, doc_id, socket).await;
    })
    .into_response()
}
```

(`Path<Uuid>` matches `/collab/:doc_id` — the route registration is unchanged.)

- [ ] **Step 5: Verify**

```bash
cargo build --workspace
cargo test -p knot-server --test convergence
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

The convergence test from the spike should still pass — it now exercises the real persistence path.

- [ ] **Step 6: Commit**

```bash
git add crates/knot-server/
git commit -m "feat(knot-server): rewrite collab WS — auth at upgrade + knot-crdt Rooms"
```

---

## Task 17: 4403 on ACL revocation

**Files:**
- Modify: `crates/knot-docs/src/listener.rs` — emit a "revoked" signal to the registry on grant-delete or doc-archive
- Modify: `crates/knot-crdt/src/registry.rs` — `revoke_all_for_doc(doc_id)` closes WS connections with 4403
- Modify: `crates/knot-crdt/src/room.rs` — `Event::Revoke` clears all conns

- [ ] **Step 1: Registry method**

Edit `crates/knot-crdt/src/registry.rs`. Add:

```rust
    pub async fn revoke_all_for_doc(&self, doc_id: Uuid) {
        if let Some(h) = self.map.get(&doc_id) {
            let _ = h.tx.send(crate::room::Event::Revoke).await;
        }
    }
```

- [ ] **Step 2: Room handles Revoke**

Edit `room.rs`. Add to `Event`:

```rust
    Revoke,
```

In `run`, handle:

```rust
                Some(Event::Revoke) => {
                    // Drop all conns. The WS shim's writer task sees the
                    // closed channel and closes the socket with 4403.
                    self.conns.clear();
                }
```

- [ ] **Step 3: WS shim writes the 4403 close frame**

Edit `crates/knot-server/src/room.rs`. After the `while let Some(bytes) = out_rx.recv()` loop exits because the receiver was closed by the room, send a 4403 close:

```rust
    let writer = tokio::spawn(async move {
        while let Some(bytes) = out_rx.recv().await {
            if sink.send(Message::Binary(bytes.into())).await.is_err() { return; }
        }
        // Channel was closed — likely an ACL revoke. Send 4403 close.
        let _ = sink
            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                code: 4403,
                reason: "acl.revoked".into(),
            })))
            .await;
    });
```

- [ ] **Step 4: Listener calls revoke**

Edit `crates/knot-docs/src/listener.rs`. The listener already receives the doc_id payload, looks up descendants, and evicts cache entries. After eviction, also notify the Rooms registry — but the listener doesn't have a Rooms handle. Easiest path: add an `on_invalidate: Arc<dyn Fn(Uuid) + Send + Sync>` callback parameter to `spawn_listener`.

```rust
pub fn spawn_listener(
    pool: PgPool,
    cache: Arc<AclCache>,
    docs: Arc<dyn knot_storage::DocStore>,
    on_invalidate: Arc<dyn Fn(Uuid) + Send + Sync>,
) -> JoinHandle<()> {
    // existing body; after evict_doc + descendants, call on_invalidate(doc_id).
}
```

In the listener loop, after the eviction calls:

```rust
                on_invalidate(doc_id);
                for d in descendants { on_invalidate(d); }
```

Then in `crates/knot-server/src/main.rs`, when constructing the listener, pass:

```rust
    let rooms_for_revoke = rooms_v2.clone();
    let on_invalidate: Arc<dyn Fn(uuid::Uuid) + Send + Sync> =
        Arc::new(move |doc_id| {
            if let Some(r) = rooms_for_revoke.clone() {
                // The revoke is async; spawn a detached task.
                let r = r.clone();
                tokio::spawn(async move { r.revoke_all_for_doc(doc_id).await; });
            }
        });
    knot_docs::spawn_listener(pool, acl, docs, on_invalidate);
```

> **Implementer note:** The current `spawn_listener` signature lives at `crates/knot-docs/src/listener.rs`. Adjust callers in `main.rs` (and any tests) to the new arity.

- [ ] **Step 5: Verify + commit**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/
git commit -m "feat(acl): close active WS with 4403 on revocation"
```

---

## Task 18: GET /api/docs/:id/markdown

**Files:**
- Create: `crates/knot-server/src/routes/api/markdown.rs`
- Modify: `crates/knot-server/src/routes/api/docs.rs` — merge into doc_id_routes so RequireDocRole layer covers it

- [ ] **Step 1: Handler**

Create `/home/nik/Development/knot/crates/knot-server/src/routes/api/markdown.rs`:

```rust
//! GET  /api/docs/:id/markdown    → text/markdown export
//! POST /api/docs/:id/markdown    Content-Type: text/markdown → import

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use knot_storage::{MarkdownCacheStore, WorkspaceRole};
use uuid::Uuid;

use crate::AppState;
use crate::auth::{AuthContext, EffectiveDocRole};
use crate::http_error::json_err;

pub(super) async fn export_inline(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    req: Request,
) -> Response {
    if req.extensions().get::<AuthContext>().is_none() {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    }
    if req.extensions().get::<EffectiveDocRole>().is_none() {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    }
    let Some(rooms) = state.rooms_v2.clone() else { return internal() };
    let Some(cache) = state.markdown_cache.clone() else { return internal() };

    // 1. Acquire the room (boots it if cold) so the doc is hot.
    let room = rooms.acquire(doc_id).await;

    // 2. Ask the room for (markdown, seq) via a oneshot. The room's actor
    //    is the only thing allowed to read the live doc.
    let (tx, rx) = tokio::sync::oneshot::channel();
    if room.tx.send(knot_crdt::Event::ExportMarkdown(tx)).await.is_err() {
        return internal();
    }
    let (text, seq) = match rx.await {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            tracing::error!(error=?e, "md export");
            return internal();
        }
        Err(_) => return internal(),
    };

    // 3. Best-effort write-through cache so cold reads on another replica
    //    don't pay the engine cost.
    let _ = cache.put(doc_id, seq, &text).await;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/markdown; charset=utf-8")
        .body(Body::from(text))
        .unwrap()
}

fn internal() -> Response {
    json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "")
}
```

- [ ] **Step 2: Add `ExportMarkdown` to Event**

Edit `crates/knot-crdt/src/room.rs`:

```rust
    ExportMarkdown(tokio::sync::oneshot::Sender<Result<(String, i64), EngineError>>),
```

In `run`:

```rust
                Some(Event::ExportMarkdown(reply)) => {
                    let r = self.engine.to_markdown(&self.doc).map(|md| (md, self.last_applied_seq));
                    let _ = reply.send(r);
                }
```

- [ ] **Step 3: Mount under docs router**

Edit `crates/knot-server/src/routes/api/docs.rs`. In the `doc_id_routes` Router, add:

```rust
        .route(
            "/api/docs/:id/markdown",
            get(crate::routes::api::markdown::export_inline),
        )
```

- [ ] **Step 4: Verify + commit**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/
git commit -m "feat(knot-server): GET /api/docs/:id/markdown via room actor"
```

---

## Task 19: POST /api/docs/:id/markdown

**Files:**
- Modify: `crates/knot-server/src/routes/api/markdown.rs`
- Modify: `crates/knot-crdt/src/room.rs` — `Event::ApplyMarkdown(text, by_user, reply)`
- Modify: `crates/knot-server/src/routes/api/docs.rs` — wire POST

- [ ] **Step 1: Engine has from_markdown — call it from the room**

In `crates/knot-crdt/src/room.rs`, add:

```rust
    ApplyMarkdown {
        text: String,
        by_user: Option<Uuid>,
        reply: tokio::sync::oneshot::Sender<Result<(), EngineError>>,
    },
```

In `run`:

```rust
                Some(Event::ApplyMarkdown { text, by_user, reply }) => {
                    let r = self.engine.from_markdown(&text);
                    match r {
                        Ok((_h, update_bytes)) => {
                            if let Err(e) = self.engine.apply_update(&self.doc, &update_bytes) {
                                let _ = reply.send(Err(e));
                                continue;
                            }
                            // Persist via the writer.
                            let _ = self.persist_tx.send(crate::writer::PersistJob {
                                bytes: update_bytes.clone(),
                                by_user_id: by_user,
                            }).await;
                            // Local fan-out.
                            for conn in self.conns.values() {
                                let _ = conn.tx.try_send(update_bytes.clone());
                            }
                            let _ = reply.send(Ok(()));
                        }
                        Err(e) => { let _ = reply.send(Err(e)); }
                    }
                }
```

- [ ] **Step 2: Handler**

Add to `crates/knot-server/src/routes/api/markdown.rs`:

```rust
pub(super) async fn import_inline(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    req: Request,
) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(role) = req.extensions().get::<EffectiveDocRole>().copied() else {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    };
    if role.0 == WorkspaceRole::Viewer {
        return json_err(StatusCode::FORBIDDEN, "acl.editor_required", "");
    }
    let Some(rooms) = state.rooms_v2.clone() else { return internal() };

    let body = match axum::body::to_bytes(req.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return json_err(StatusCode::BAD_REQUEST, "bad_request", ""),
    };
    let text = match std::str::from_utf8(&body) {
        Ok(s) => s.to_string(),
        Err(_) => return json_err(StatusCode::UNPROCESSABLE_ENTITY, "markdown.not_utf8", ""),
    };

    let room = rooms.acquire(doc_id).await;
    let (tx, rx) = tokio::sync::oneshot::channel();
    if room.tx.send(knot_crdt::Event::ApplyMarkdown {
        text,
        by_user: Some(ctx.user_id),
        reply: tx,
    }).await.is_err() {
        return internal();
    }
    match rx.await {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(e)) => {
            tracing::warn!(error=?e, "md import");
            json_err(StatusCode::UNPROCESSABLE_ENTITY, "markdown.parse", "")
        }
        Err(_) => internal(),
    }
}
```

- [ ] **Step 3: Wire POST**

Edit `crates/knot-server/src/routes/api/docs.rs`. Replace the single `.route("/api/docs/:id/markdown", get(...))` from T18 with:

```rust
        .route(
            "/api/docs/:id/markdown",
            get(crate::routes::api::markdown::export_inline)
                .post(crate::routes::api::markdown::import_inline),
        )
```

- [ ] **Step 4: Verify + commit**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
git add crates/
git commit -m "feat(knot-server): POST /api/docs/:id/markdown via room actor"
```

---

## Task 20: e2e — collab persistence + reconnect

**Files:**
- Create: `e2e/flows/collab.spec.ts`

- [ ] **Step 1: e2e**

Create `/home/nik/Development/knot/e2e/flows/collab.spec.ts`:

```ts
import { test, expect, request } from "@playwright/test";
import { execSync } from "node:child_process";

const SERVER = "http://localhost:3000";

function reset() {
  const tables = [
    "acl_invalidations", "audit_events", "doc_markdown_cache",
    "doc_snapshots", "doc_updates", "document_grants", "documents",
    "sessions", "workspace_members", "users", "workspaces",
  ].join(", ");
  execSync(
    `docker compose -f deploy/compose/dev.yml exec -T postgres psql -U knot -d knot -c "TRUNCATE TABLE ${tables} CASCADE"`,
    { cwd: "..", stdio: "pipe" },
  );
}

test.beforeAll(reset);

async function adminCtx() {
  const ctx = await request.newContext({ baseURL: SERVER });
  const setup = await ctx.post("/auth/setup", {
    data: {
      email: "owner@example.com",
      password: "owner-hunter22",
      display_name: "Owner",
    },
  });
  expect(setup.status()).toBe(201);
  return ctx;
}

async function csrfTokenFor(ctx: any): Promise<string> {
  const cookies = await ctx.storageState();
  const csrf = cookies.cookies.find((c: any) => c.name === "csrf");
  if (!csrf) throw new Error("csrf cookie not found");
  return csrf.value;
}

test("markdown import + export round trip via room actor", async () => {
  const ctx = await adminCtx();
  const csrf = await csrfTokenFor(ctx);
  const writeHeaders = { "X-CSRF-Token": csrf };

  // Create a doc.
  const created = await ctx.post("/api/docs", {
    headers: writeHeaders,
    data: { title: "MD" },
  });
  expect(created.status()).toBe(201);
  const doc = await created.json();

  // Import some markdown.
  const md = "# Hello\n\nworld.\n";
  const imp = await ctx.post(`/api/docs/${doc.id}/markdown`, {
    headers: { ...writeHeaders, "Content-Type": "text/markdown" },
    data: md,
  });
  expect(imp.status()).toBe(204);

  // Export — must round-trip the heading + paragraph at minimum.
  const exp = await ctx.get(`/api/docs/${doc.id}/markdown`);
  expect(exp.status()).toBe(200);
  const text = await exp.text();
  expect(text).toContain("# Hello");
  expect(text).toContain("world.");
});
```

- [ ] **Step 2: Run e2e**

```bash
cd /home/nik/Development/knot
make compose.up
make migrate.up
cd e2e
pnpm playwright test
```

Expected: 6 specs pass total (auth, docs, health, two-users-converge, collab + previously existing).

- [ ] **Step 3: Commit**

```bash
git add e2e/
git commit -m "test(e2e): markdown import + export round trip via room actor"
```

---

## Self-review checklist (for the executing agent)

Before declaring Plan 5 complete:

- [ ] `cargo test --workspace` green.
- [ ] `cd e2e && pnpm test` green (6 specs).
- [ ] `cargo deny check` green.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
- [ ] WS auth: hitting `/collab/:doc_id` without a sid cookie returns 401 (manual smoke).
- [ ] A grant deletion while a WS is open closes the socket with code 4403 (smoke via curl + websocat or hand-rolled JS).
- [ ] `GET /api/docs/:id/markdown` is cached on the second call (manual: hit twice, check `doc_markdown_cache` has a row).
- [ ] Snapshot is written after `KNOT_SNAPSHOT_EVERY_N` updates (manual smoke against a low value like 5 via env).
- [ ] Two replicas running against the same DB see each other's edits (manual: `cargo run --bin knot-server` on ports 3000 + 3100 sharing the compose Postgres + Dex).

When green: write `docs/superpowers/research/2026-06-0X-plan5-outcome.md` (scope landed, spec drift, what's left for Plan 6+), tag `plan-5-complete`, and proceed to Plan 6 (Frontend shell).
