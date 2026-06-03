# File Uploads & Attachments Implementation Plan (Plan 13)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let users drop images and attachments into a doc. Two server-side blob backends shipped behind a single `BlobStore` trait: **Postgres bytea** (default — same DB as everything else, single backup story) and **S3-compatible** (MinIO/R2/native AWS — for deployments that outgrow bytea). Hard cap of **10 MB** per upload. ACL inherits from the doc: if you can view the doc, you can GET the blob; if you can edit the doc, you can POST a new blob to it.

**Architecture:**

- **`BlobStore` trait** in `knot-storage` with `put`/`get`/`head`/`delete` — just byte streaming. Metadata (workspace_id, doc_id, content_type, byte_size, sha256, created_by) lives in a `blobs` table regardless of backend. Two implementations:
  - `PgBytesStore` — bytea in a separate `blob_bytes` table, keyed by blob_id. Default. 10 MB cap fits comfortably.
  - `S3Store` — uses `aws-sdk-s3` 1.x (compatible with MinIO + R2). Feature-flagged behind `s3` cargo feature so default builds don't pull the AWS SDK.
- **Tiptap node types** — `@tiptap/extension-image` for images + a custom `Attachment` node for arbitrary files. The Attachment node renders as a download link with filename + size.
- **Upload flow** — single `POST /api/docs/:doc_id/blobs` (multipart). Server validates size + content-type, computes sha256, stores bytes via the chosen `BlobStore`, returns the metadata. Client gets back a blob URL `/api/blobs/:id` and inserts the appropriate Tiptap node.
- **ACL** — `GET /api/blobs/:id` re-runs `effective_role(workspace, doc, user)`. Cache-Control: private to keep CDNs honest.

**Tech Stack:**
- Rust: `multer` 3.x (axum 0.7 native multipart is fiddly), `sha2`, `aws-sdk-s3 = 1.x` (feature-gated).
- TS: `@tiptap/extension-image` (small). The Attachment node is hand-rolled (~80 LOC) — no extra dep.
- Migration: one new migration adds two tables.

**Predecessor:** Plan 12 (production hardening, HEAD `6b30ecf`).

**Spec coverage:**

| Spec section | Tasks |
|---|---|
| §13.1 BlobStore trait + Pg bytea backend | T2, T3 |
| §13.2 S3-compatible backend (feature-gated) | T4 |
| §13.3 POST /api/docs/:doc_id/blobs (multipart, 10 MB cap, ACL) | T5 |
| §13.4 GET /api/blobs/:id (ACL re-check) | T6 |
| §13.5 DELETE /api/blobs/:id (editor+ role required) | T7 |
| §13.6 Tiptap Image extension + drop-to-upload | T9, T11 |
| §13.7 Tiptap Attachment node (custom) | T10, T11 |
| §13.8 Helm values: blob.backend + S3 config | T12 |
| §13.9 Server integration tests (5+) | T8 |
| §13.10 e2e for image drop | T13 |

**Out of scope:**

- **Image transcoding / thumbnails.** v0.1 serves the original bytes. Thumbnails are a CDN concern.
- **Per-blob versioning.** Yjs already gives you doc history; blobs are content-addressed by sha256 only.
- **Garbage collection of orphan blobs.** When a doc is archived, blobs cascade-delete via FK. Orphans from copy/paste are a follow-up GC job.
- **Pre-signed S3 URLs.** Direct upload from browser to S3 is a network-perf win but adds a CORS + signature surface; defer.
- **Antivirus / Office-file inspection.** Out of scope for v0.1.
- **Multipart resumable uploads.** 10 MB cap means single-request is fine.
- **Per-user storage quotas.** Workspace-level only via the bytea size column.

---

## File map

```
migrations/
└── <ts>_blobs.sql                                     (new) blobs + blob_bytes tables

crates/knot-storage/
├── Cargo.toml                                          (modify) +sha2, +aws-sdk-s3 (s3 feature), +async-trait
└── src/
    ├── lib.rs                                          (modify) re-export blobs
    ├── blobs.rs                                        (new) BlobStore trait + Blob record types
    ├── blobs/pg.rs                                     (new) PgBytesStore + metadata operations
    └── blobs/s3.rs                                     (new) S3Store behind #[cfg(feature = "s3")]

crates/knot-server/
├── Cargo.toml                                          (modify) +multer
├── src/
│   ├── lib.rs                                          (modify) wire blobs into AppState
│   └── routes/api/
│       └── blobs.rs                                    (new) POST /api/docs/:id/blobs, GET/DELETE /api/blobs/:id
└── tests/
    └── blobs_integration.rs                            (new) upload + ACL + size cap + content-type

web/
├── package.json                                        (modify) +@tiptap/extension-image
└── src/
    ├── lib/blobs.api.ts                                (new) blobsApi.upload(file, docId)
    └── features/editor/
        ├── extensions.ts                               (modify) +Image, +Attachment
        ├── nodes/AttachmentNode.tsx                    (new) custom Tiptap node
        └── KnotEditor.tsx                              (modify) drop handler routes images vs others

e2e/flows/
└── upload-image.spec.ts                                (new) drop a PNG, see <img> appear, reload preserves

deploy/helm/knot/
├── values.yaml                                         (modify) +blob.backend + s3 block
├── values.schema.json                                  (modify) new keys
└── templates/configmap.yaml                            (modify) +KNOT_BLOB_BACKEND + S3 vars

docs/
└── superpowers/research/2026-06-0X-plan13-outcome.md   (new)
```

---

## Conventions

- **Content-types accepted:** `image/png`, `image/jpeg`, `image/gif`, `image/webp` go to the Image node. Anything else (within reason — block executables) goes to the Attachment node. Server allow-list is whitelist-based for images, blocklist for attachments.
- **Multipart parsing:** stream into a `Vec<u8>` with a 10 MB cap enforced before reading the body. `multer::Multipart::next_field()` + `field.bytes()` — straightforward.
- **sha256:** server-computed. Stored as `bytea`. Doesn't affect dedup in v0.1 but reserves the option.
- **Blob URLs in the editor:** absolute `/api/blobs/:id` so they survive copy/paste between docs and work after deploy.
- **ACL re-check on GET:** every request. No caching beyond `Cache-Control: private, max-age=60` so a revoked user loses access within a minute.
- **S3 feature flag:** `cargo build` (no features) gives only `PgBytesStore`. `cargo build --features knot-server/s3` (or workspace `--features s3`) pulls in the AWS SDK. CI builds both.

---

## Task overview

| # | Title | LOC ≈ |
|---|---|---|
| 1 | Migration: blobs + blob_bytes tables | 50 |
| 2 | knot-storage: BlobStore trait + Blob types | 120 |
| 3 | knot-storage: PgBytesStore implementation | 140 |
| 4 | knot-storage: S3Store (feature-gated) | 180 |
| 5 | knot-server: POST /api/docs/:id/blobs | 200 |
| 6 | knot-server: GET /api/blobs/:id (with ACL) | 80 |
| 7 | knot-server: DELETE /api/blobs/:id | 60 |
| 8 | Server integration tests | 220 |
| 9 | web: blobsApi.upload + Image extension wired | 140 |
| 10 | web: Attachment custom Tiptap node | 160 |
| 11 | web: Editor drop handler routes image vs file | 100 |
| 12 | Helm: blob backend values + ConfigMap wiring | 100 |
| 13 | e2e: drop an image, reload, persists | 120 |
| 14 | Outcome doc | 0 |

---

## Task 1: Migration

**Files:**
- Create: `migrations/<ts>_blobs.sql` (use `make migrate.create NAME=blobs`)

- [ ] **Step 1: Scaffold**

```bash
make migrate.create NAME=blobs
```

- [ ] **Step 2: Schema**

```sql
-- blobs
-- Metadata for every blob attached to a doc. The bytes themselves live
-- in either blob_bytes (Postgres backend) or an S3 bucket (S3 backend).
CREATE TABLE blobs (
  id           UUID PRIMARY KEY,
  workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  doc_id       UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  content_type TEXT NOT NULL,
  byte_size    BIGINT NOT NULL CHECK (byte_size > 0 AND byte_size <= 10485760),
  sha256       BYTEA NOT NULL,
  original_name TEXT,
  created_by   UUID NOT NULL REFERENCES users(id),
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX blobs_doc_idx ON blobs(doc_id);
CREATE INDEX blobs_workspace_idx ON blobs(workspace_id);

-- blob_bytes (Postgres backend only).
-- Separate table so the metadata stays light when listing.
CREATE TABLE blob_bytes (
  blob_id UUID PRIMARY KEY REFERENCES blobs(id) ON DELETE CASCADE,
  bytes   BYTEA NOT NULL
);
```

- [ ] **Step 3: Commit**

```bash
git add migrations/
git commit -m "feat(migrations): blobs + blob_bytes tables"
```

---

## Task 2: BlobStore trait

**Files:**
- Modify: `crates/knot-storage/Cargo.toml`
- Create: `crates/knot-storage/src/blobs.rs`
- Modify: `crates/knot-storage/src/lib.rs`

- [ ] **Step 1: Dependencies**

Add to `crates/knot-storage/Cargo.toml`:

```toml
async-trait = "0.1"
sha2 = "0.10"

[features]
default = []
s3 = ["aws-sdk-s3", "aws-config"]

[dependencies.aws-sdk-s3]
version = "1"
optional = true
default-features = false
features = ["behavior-version-latest", "rt-tokio", "rustls"]

[dependencies.aws-config]
version = "1"
optional = true
default-features = false
features = ["behavior-version-latest", "rt-tokio", "rustls"]
```

(Adjust to whatever AWS SDK version is current. Defer features to the implementation in T4.)

- [ ] **Step 2: Trait + record types**

Create `crates/knot-storage/src/blobs.rs`:

```rust
use async_trait::async_trait;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum BlobStoreError {
    #[error("not found")]
    NotFound,
    #[error("backend: {0}")]
    Backend(String),
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
}

pub type Result<T> = std::result::Result<T, BlobStoreError>;

#[derive(Debug, Clone)]
pub struct BlobMetadata {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub doc_id: Uuid,
    pub content_type: String,
    pub byte_size: i64,
    pub sha256: Vec<u8>,
    pub original_name: Option<String>,
    pub created_by: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, id: Uuid, bytes: &[u8], content_type: &str) -> Result<()>;
    async fn get(&self, id: Uuid) -> Result<Vec<u8>>;
    async fn delete(&self, id: Uuid) -> Result<()>;
}

/// Metadata operations — shared across all backends. Always backed by Postgres.
pub struct BlobMeta {
    pool: PgPool,
}

impl BlobMeta {
    pub fn new(pool: PgPool) -> Self { Self { pool } }

    pub async fn insert(&self, m: &BlobMetadata) -> Result<()> {
        sqlx::query(
            "INSERT INTO blobs (id, workspace_id, doc_id, content_type, byte_size, sha256, original_name, created_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(m.id)
        .bind(m.workspace_id)
        .bind(m.doc_id)
        .bind(&m.content_type)
        .bind(m.byte_size)
        .bind(&m.sha256)
        .bind(&m.original_name)
        .bind(m.created_by)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn find(&self, id: Uuid) -> Result<Option<BlobMetadata>> {
        let row = sqlx::query_as::<_, BlobRow>(
            "SELECT id, workspace_id, doc_id, content_type, byte_size, sha256, original_name, created_by, created_at \
             FROM blobs WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    pub async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM blobs WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct BlobRow {
    id: Uuid,
    workspace_id: Uuid,
    doc_id: Uuid,
    content_type: String,
    byte_size: i64,
    sha256: Vec<u8>,
    original_name: Option<String>,
    created_by: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<BlobRow> for BlobMetadata {
    fn from(r: BlobRow) -> Self {
        Self {
            id: r.id,
            workspace_id: r.workspace_id,
            doc_id: r.doc_id,
            content_type: r.content_type,
            byte_size: r.byte_size,
            sha256: r.sha256,
            original_name: r.original_name,
            created_by: r.created_by,
            created_at: r.created_at,
        }
    }
}
```

In `crates/knot-storage/src/lib.rs`, add:

```rust
pub mod blobs;
pub use blobs::{BlobMeta, BlobMetadata, BlobStore, BlobStoreError};
```

- [ ] **Step 3: Verify**

```bash
cargo check -p knot-storage
cargo clippy -p knot-storage --all-targets --all-features -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add crates/knot-storage/
git commit -m "feat(knot-storage): BlobStore trait + Postgres-backed BlobMeta"
```

---

## Task 3: PgBytesStore

**Files:**
- Create: `crates/knot-storage/src/blobs/pg.rs`
- Modify: `crates/knot-storage/src/blobs.rs` — re-export

- [ ] **Step 1: Implementation**

```rust
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use super::{BlobStore, BlobStoreError, Result};

pub struct PgBytesStore {
    pool: PgPool,
}

impl PgBytesStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl BlobStore for PgBytesStore {
    async fn put(&self, id: Uuid, bytes: &[u8], _content_type: &str) -> Result<()> {
        sqlx::query("INSERT INTO blob_bytes (blob_id, bytes) VALUES ($1, $2)")
            .bind(id)
            .bind(bytes)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Vec<u8>> {
        let row: Option<(Vec<u8>,)> =
            sqlx::query_as("SELECT bytes FROM blob_bytes WHERE blob_id = $1")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        row.map(|(b,)| b).ok_or(BlobStoreError::NotFound)
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        sqlx::query("DELETE FROM blob_bytes WHERE blob_id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
```

In `blobs.rs`, add at the bottom:

```rust
pub mod pg;
pub use pg::PgBytesStore;
```

- [ ] **Step 2: Verify + commit**

```bash
cargo check -p knot-storage
cargo clippy -p knot-storage --all-targets --all-features -- -D warnings
git add crates/knot-storage/
git commit -m "feat(knot-storage): PgBytesStore — Postgres bytea blob backend"
```

---

## Task 4: S3Store (feature-gated)

**Files:**
- Create: `crates/knot-storage/src/blobs/s3.rs`
- Modify: `crates/knot-storage/src/blobs.rs` — feature-gated re-export

- [ ] **Step 1: Implementation**

```rust
//! S3-compatible blob backend (AWS S3, MinIO, R2, ...).
//!
//! Feature-gated behind `s3` so default builds don't pull in the AWS SDK.

use async_trait::async_trait;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use uuid::Uuid;

use super::{BlobStore, BlobStoreError, Result};

pub struct S3Store {
    client: Client,
    bucket: String,
    prefix: String,
}

impl S3Store {
    /// Build a store using ambient AWS config (KNOT_S3_ENDPOINT, region, creds).
    pub fn new(client: Client, bucket: String, prefix: String) -> Self {
        Self { client, bucket, prefix }
    }

    fn key(&self, id: Uuid) -> String {
        if self.prefix.is_empty() { id.to_string() }
        else { format!("{}/{}", self.prefix.trim_end_matches('/'), id) }
    }
}

#[async_trait]
impl BlobStore for S3Store {
    async fn put(&self, id: Uuid, bytes: &[u8], content_type: &str) -> Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(self.key(id))
            .body(ByteStream::from(bytes.to_vec()))
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| BlobStoreError::Backend(format!("s3 put: {e}")))?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Vec<u8>> {
        let resp = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(self.key(id))
            .send()
            .await
            .map_err(|e| {
                let s = format!("{e}");
                if s.contains("NoSuchKey") { BlobStoreError::NotFound }
                else { BlobStoreError::Backend(format!("s3 get: {e}")) }
            })?;
        let bytes = resp.body.collect().await
            .map_err(|e| BlobStoreError::Backend(format!("s3 body: {e}")))?;
        Ok(bytes.to_vec())
    }

    async fn delete(&self, id: Uuid) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(self.key(id))
            .send()
            .await
            .map_err(|e| BlobStoreError::Backend(format!("s3 delete: {e}")))?;
        Ok(())
    }
}
```

In `blobs.rs`:

```rust
#[cfg(feature = "s3")]
pub mod s3;
#[cfg(feature = "s3")]
pub use s3::S3Store;
```

- [ ] **Step 2: Verify**

```bash
cargo check -p knot-storage                # default features — should compile
cargo check -p knot-storage --features s3  # s3 build — should compile
cargo clippy -p knot-storage --all-targets --all-features -- -D warnings
```

If the AWS SDK builds explode, downgrade or pin to a known-good version. Don't fight unbounded compile times — the s3 feature is opt-in for a reason.

- [ ] **Step 3: Commit**

```bash
git add crates/knot-storage/
git commit -m "feat(knot-storage): S3Store backend behind 's3' feature"
```

---

## Task 5: POST /api/docs/:doc_id/blobs

**Files:**
- Modify: `crates/knot-server/Cargo.toml` — `+multer`
- Modify: `crates/knot-server/src/lib.rs` — add `blob_store: Option<Arc<dyn BlobStore>>` + `blob_meta: Option<Arc<BlobMeta>>` to `AppState`, wire in `main.rs`
- Create: `crates/knot-server/src/routes/api/blobs.rs`

- [ ] **Step 1: AppState**

In `crates/knot-server/src/lib.rs`, add to `AppState`:

```rust
pub blob_store: Option<Arc<dyn knot_storage::BlobStore>>,
pub blob_meta:  Option<Arc<knot_storage::BlobMeta>>,
```

Init to `None` in `AppState::with_pool` and `AppState::in_memory`. Wire in `main.rs` when a pool exists — default to `PgBytesStore`; when `KNOT_BLOB_BACKEND=s3` and the `s3` feature is enabled, build `S3Store` from env (`KNOT_S3_BUCKET`, `KNOT_S3_ENDPOINT`, `KNOT_S3_PREFIX`, `KNOT_S3_REGION`).

- [ ] **Step 2: Multipart upload handler**

Create `crates/knot-server/src/routes/api/blobs.rs`:

```rust
//! Blob upload / download / delete.
//!
//! POST   /api/docs/:doc_id/blobs           multipart, returns BlobMetadata
//! GET    /api/blobs/:id                    streams bytes, ACL-checked
//! DELETE /api/blobs/:id                    editor+ on parent doc

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use multer::{Constraints, Multipart, SizeLimit};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::http_error::json_err;
use crate::AppState;

const MAX_BLOB_BYTES: u64 = 10 * 1024 * 1024;
const ALLOWED_IMAGE_TYPES: &[&str] = &[
    "image/png", "image/jpeg", "image/gif", "image/webp",
];
const BLOCKED_PREFIXES: &[&str] = &[
    "application/x-executable", "application/x-msdownload",
    "application/x-msdos-program", "application/x-mach-binary",
];

#[derive(serde::Serialize)]
struct BlobResponse {
    id: String,
    doc_id: String,
    content_type: String,
    byte_size: i64,
    url: String,
    original_name: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/docs/:doc_id/blobs", post(upload))
        .route("/api/blobs/:id", get(download).delete(delete_blob))
}

async fn upload(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    req: Request,
) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };

    // ACL: must have editor or owner on the doc.
    let Some(acl) = state.acl.clone() else { return internal(); };
    match acl.effective_role(ctx.workspace_id, doc_id, ctx.user_id).await {
        Ok(Some(role)) if matches!(role, knot_storage::WorkspaceRole::Owner | knot_storage::WorkspaceRole::Editor) => {}
        Ok(_) => return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "editor role required"),
        Err(_) => return internal(),
    }

    // Extract boundary from content-type.
    let Some(content_type) = req.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) else {
        return json_err(StatusCode::BAD_REQUEST, "blob.no_content_type", "");
    };
    let Ok(boundary) = multer::parse_boundary(content_type) else {
        return json_err(StatusCode::BAD_REQUEST, "blob.bad_multipart", "");
    };

    let stream = req.into_body().into_data_stream();
    let constraints = Constraints::new()
        .size_limit(SizeLimit::new().whole_stream(MAX_BLOB_BYTES));
    let mut mp = Multipart::with_constraints(stream, boundary, constraints);

    // Read first field named "file".
    let Some(field) = mp.next_field().await.ok().flatten() else {
        return json_err(StatusCode::BAD_REQUEST, "blob.missing_file", "");
    };
    let original_name = field.file_name().map(|s| s.to_string());
    let field_ct = field.content_type().map(|m| m.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    if BLOCKED_PREFIXES.iter().any(|p| field_ct.starts_with(p)) {
        return json_err(StatusCode::UNSUPPORTED_MEDIA_TYPE, "blob.blocked_type", &field_ct);
    }

    let bytes = match field.bytes().await {
        Ok(b) => b,
        Err(e) if matches!(e, multer::Error::StreamSizeExceeded { .. }) => {
            return json_err(StatusCode::PAYLOAD_TOO_LARGE, "blob.too_large", "10 MB cap");
        }
        Err(_) => return json_err(StatusCode::BAD_REQUEST, "blob.read_error", ""),
    };
    if bytes.is_empty() {
        return json_err(StatusCode::BAD_REQUEST, "blob.empty", "");
    }

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha256 = hasher.finalize().to_vec();

    let blob_id = Uuid::new_v4();
    let meta = knot_storage::BlobMetadata {
        id: blob_id,
        workspace_id: ctx.workspace_id,
        doc_id,
        content_type: field_ct.clone(),
        byte_size: bytes.len() as i64,
        sha256,
        original_name: original_name.clone(),
        created_by: ctx.user_id,
        created_at: chrono::Utc::now(),
    };

    let Some(store) = state.blob_store.clone() else { return internal(); };
    let Some(blobs) = state.blob_meta.clone() else { return internal(); };

    if let Err(e) = store.put(blob_id, &bytes, &field_ct).await {
        tracing::error!(error=?e, "blob put");
        return internal();
    }
    if let Err(e) = blobs.insert(&meta).await {
        let _ = store.delete(blob_id).await;
        tracing::error!(error=?e, "blob meta insert");
        return internal();
    }

    (StatusCode::CREATED, Json(BlobResponse {
        id: meta.id.to_string(),
        doc_id: meta.doc_id.to_string(),
        content_type: meta.content_type,
        byte_size: meta.byte_size,
        url: format!("/api/blobs/{}", meta.id),
        original_name: meta.original_name,
    })).into_response()
}

async fn download(State(state): State<AppState>, Path(id): Path<Uuid>, req: Request) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(blobs) = state.blob_meta.clone() else { return internal(); };
    let Some(store) = state.blob_store.clone() else { return internal(); };
    let Some(acl) = state.acl.clone() else { return internal(); };

    let meta = match blobs.find(id).await {
        Ok(Some(m)) => m,
        Ok(None) => return json_err(StatusCode::NOT_FOUND, "blob.not_found", ""),
        Err(_) => return internal(),
    };
    match acl.effective_role(meta.workspace_id, meta.doc_id, ctx.user_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return json_err(StatusCode::FORBIDDEN, "acl.no_grant", ""),
        Err(_) => return internal(),
    }

    let bytes = match store.get(id).await {
        Ok(b) => b,
        Err(knot_storage::BlobStoreError::NotFound) => return json_err(StatusCode::NOT_FOUND, "blob.not_found", ""),
        Err(_) => return internal(),
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, meta.content_type)
        .header(header::CACHE_CONTROL, "private, max-age=60")
        .header(header::CONTENT_LENGTH, meta.byte_size)
        .body(Body::from(bytes))
        .unwrap()
}

async fn delete_blob(State(state): State<AppState>, Path(id): Path<Uuid>, req: Request) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(blobs) = state.blob_meta.clone() else { return internal(); };
    let Some(store) = state.blob_store.clone() else { return internal(); };
    let Some(acl) = state.acl.clone() else { return internal(); };

    let meta = match blobs.find(id).await {
        Ok(Some(m)) => m,
        Ok(None) => return json_err(StatusCode::NOT_FOUND, "blob.not_found", ""),
        Err(_) => return internal(),
    };
    match acl.effective_role(meta.workspace_id, meta.doc_id, ctx.user_id).await {
        Ok(Some(role)) if matches!(role, knot_storage::WorkspaceRole::Owner | knot_storage::WorkspaceRole::Editor) => {}
        _ => return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "editor role required"),
    }

    let _ = store.delete(id).await;
    let _ = blobs.delete(id).await;
    StatusCode::NO_CONTENT.into_response()
}

fn internal() -> Response { json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "") }
```

Add `multer = "3"` to `crates/knot-server/Cargo.toml`. Wire the router into `lib.rs` next to the existing API routers.

- [ ] **Step 3: Verify**

```bash
cargo check -p knot-server
cargo clippy -p knot-server --all-targets --all-features -- -D warnings
```

- [ ] **Step 4: Commit**

```bash
git add crates/knot-server/
git commit -m "feat(knot-server): POST /api/docs/:doc_id/blobs + GET/DELETE /api/blobs/:id"
```

(Tasks 5+6+7 are all in this commit since they share the file.)

---

## Tasks 6 + 7

Merged into Task 5 — same file, same router. No separate commits needed.

---

## Task 8: Server integration tests

**Files:**
- Create: `crates/knot-server/tests/blobs_integration.rs`

- [ ] **Step 1: Scenarios**

Cases (use the same `state_with_seeded_user` + `router_with_state` helpers as `auth_local_integration.rs`):

1. Owner uploads a 1 KB PNG → 201, response has `id` + `url` + byte_size=N.
2. GET the returned URL → 200, content-type `image/png`, body matches.
3. Owner uploads 11 MB → 413 `blob.too_large`.
4. Owner uploads `.exe` (content-type `application/x-msdos-program`) → 415 `blob.blocked_type`.
5. Viewer (workspace-level viewer) tries to upload → 403 `acl.no_grant`.
6. Anon (no sid cookie) GET → 401 `auth.session_required`.
7. Owner DELETE the blob → 204; subsequent GET → 404.

Build multipart bodies by hand (mirror the format `multer` parses):

```rust
fn multipart_body(boundary: &str, filename: &str, ct: &str, bytes: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n").as_bytes());
    body.extend_from_slice(format!("Content-Type: {ct}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}
```

Wire `state.blob_store` + `state.blob_meta` in the test helper.

- [ ] **Step 2: Run + commit**

```bash
make compose.up
cargo nextest run -p knot-server --test blobs_integration
git add crates/knot-server/
git commit -m "test(knot-server): blob upload/download/ACL/size/blocked"
```

---

## Task 9: Frontend API client + Image extension

**Files:**
- Modify: `web/package.json` — `+@tiptap/extension-image`
- Create: `web/src/lib/blobs.api.ts`
- Modify: `web/src/features/editor/extensions.ts` — add Image

- [ ] **Step 1: Dep**

```bash
cd web && pnpm add @tiptap/extension-image
```

- [ ] **Step 2: API client**

```ts
// web/src/lib/blobs.api.ts
import { type ApiError, type ApiResult } from "./api";
import { readCookie } from "./csrf";

export type BlobResponse = {
  id: string;
  doc_id: string;
  content_type: string;
  byte_size: number;
  url: string;
  original_name: string | null;
};

export const blobsApi = {
  async upload(docId: string, file: File): Promise<ApiResult<BlobResponse>> {
    const fd = new FormData();
    fd.append("file", file, file.name);
    const headers: Record<string, string> = {};
    const csrf = readCookie("csrf");
    if (csrf) headers["X-CSRF-Token"] = csrf;
    const res = await fetch(`/api/docs/${encodeURIComponent(docId)}/blobs`, {
      method: "POST",
      credentials: "include",
      headers,
      body: fd,
    });
    const text = await res.text();
    if (!res.ok) {
      try {
        const env = JSON.parse(text) as { error?: Partial<ApiError> };
        return {
          error: {
            code: env.error?.code ?? "http_error",
            message: env.error?.message ?? `HTTP ${res.status}`,
            details: env.error?.details ?? {},
            status: res.status,
          },
        };
      } catch {
        return { error: { code: "http_error", message: `HTTP ${res.status}`, details: {}, status: res.status } };
      }
    }
    return { ok: JSON.parse(text) as BlobResponse };
  },
};
```

- [ ] **Step 3: Image extension**

```ts
import Image from "@tiptap/extension-image";
// add to the createExtensions return list:
Image.configure({ inline: false, allowBase64: false }),
```

- [ ] **Step 4: Verify + commit**

```bash
pnpm tsc && pnpm lint
git add web/
git commit -m "feat(web): blobsApi.upload + Tiptap Image extension"
```

---

## Task 10: Attachment Tiptap node

**Files:**
- Create: `web/src/features/editor/nodes/AttachmentNode.tsx`
- Modify: `web/src/features/editor/extensions.ts`

- [ ] **Step 1: Custom node**

```tsx
import { Node, mergeAttributes } from "@tiptap/core";
import { NodeViewWrapper, ReactNodeViewRenderer } from "@tiptap/react";

const Renderer = ({ node }: { node: { attrs: { url: string; name: string; size: number; contentType: string } } }) => (
  <NodeViewWrapper as="div" data-testid="attachment-node" style={{
    display: "inline-flex", gap: 8, padding: 8,
    border: "1px solid #e5e5e5", borderRadius: 6,
    background: "#fafafa", alignItems: "center",
  }}>
    <span aria-hidden>📎</span>
    <a href={node.attrs.url} target="_blank" rel="noopener noreferrer" download={node.attrs.name}>
      {node.attrs.name}
    </a>
    <span style={{ color: "#888", fontSize: 12 }}>({Math.round(node.attrs.size / 1024)} KB)</span>
  </NodeViewWrapper>
);

export const Attachment = Node.create({
  name: "attachment",
  group: "block",
  atom: true,
  addAttributes() {
    return {
      url:         { default: "" },
      name:        { default: "file" },
      size:        { default: 0 },
      contentType: { default: "application/octet-stream" },
    };
  },
  parseHTML() { return [{ tag: 'div[data-attachment]' }]; },
  renderHTML({ HTMLAttributes }) {
    return ["div", mergeAttributes(HTMLAttributes, { "data-attachment": "true" })];
  },
  addNodeView() { return ReactNodeViewRenderer(Renderer); },
});
```

- [ ] **Step 2: Register**

In `extensions.ts`:

```ts
import { Attachment } from "./nodes/AttachmentNode";
// add to the createExtensions return list:
Attachment,
```

- [ ] **Step 3: Verify + commit**

```bash
pnpm tsc && pnpm lint && pnpm test
git add web/
git commit -m "feat(web): Attachment Tiptap node (custom)"
```

---

## Task 11: Editor drop handler

**Files:**
- Modify: `web/src/features/editor/KnotEditor.tsx`

- [ ] **Step 1: handleDrop**

In the Tiptap `useEditor` call inside `EditorBody`, add `editorProps`:

```tsx
const isImage = (f: File) => /^image\/(png|jpe?g|gif|webp)$/.test(f.type);

editorProps: {
  handleDrop(view, event, _slice, _moved) {
    const files = Array.from(event.dataTransfer?.files ?? []);
    if (files.length === 0) return false;
    event.preventDefault();
    void uploadAndInsert(files);
    return true;
  },
  handlePaste(view, event) {
    const files = Array.from(event.clipboardData?.files ?? []);
    if (files.length === 0) return false;
    void uploadAndInsert(files);
    return true;
  },
},
```

And in the component body:

```tsx
const uploadAndInsert = async (files: File[]) => {
  for (const f of files) {
    const r = await blobsApi.upload(docId, f);
    if ("error" in r) {
      notify("error", r.error.code === "blob.too_large"
        ? "File is too large (10 MB cap)."
        : r.error.code === "blob.blocked_type"
          ? "File type not allowed."
          : "Upload failed.");
      continue;
    }
    const blob = r.ok;
    if (isImage(f)) {
      editor?.chain().focus().setImage({ src: blob.url }).run();
    } else {
      editor?.chain().focus().insertContent({
        type: "attachment",
        attrs: { url: blob.url, name: blob.original_name ?? f.name, size: blob.byte_size, contentType: blob.content_type },
      }).run();
    }
  }
};
```

Import `blobsApi` + `useUi` for `notify`. Pass `docId` through to `EditorBody` since drop wires need it.

- [ ] **Step 2: Verify + commit**

```bash
pnpm tsc && pnpm lint
git add web/
git commit -m "feat(web): drop/paste handler — upload + insert image or attachment"
```

---

## Task 12: Helm values

**Files:**
- Modify: `deploy/helm/knot/values.yaml`
- Modify: `deploy/helm/knot/values.schema.json`
- Modify: `deploy/helm/knot/templates/configmap.yaml`

- [ ] **Step 1: values.yaml**

```yaml
blob:
  backend: postgres          # postgres | s3
  s3:
    bucket: ""
    endpoint: ""             # e.g. http://minio.svc.cluster.local:9000
    region: us-east-1
    prefix: ""
    # AWS creds via existingSecret keys AWS_ACCESS_KEY_ID + AWS_SECRET_ACCESS_KEY
    existingSecretName: ""
```

- [ ] **Step 2: configmap.yaml** — append:

```yaml
  KNOT_BLOB_BACKEND: {{ .Values.blob.backend | quote }}
  {{- if eq .Values.blob.backend "s3" }}
  KNOT_S3_BUCKET:   {{ required "blob.s3.bucket is required when backend=s3" .Values.blob.s3.bucket | quote }}
  KNOT_S3_ENDPOINT: {{ .Values.blob.s3.endpoint | quote }}
  KNOT_S3_REGION:   {{ .Values.blob.s3.region | quote }}
  KNOT_S3_PREFIX:   {{ .Values.blob.s3.prefix | quote }}
  {{- end }}
```

For deployment.yaml — if `blob.backend == s3` AND `existingSecretName` is set, add an envFrom referencing it.

- [ ] **Step 3: schema**

```json
"blob": {
  "type": "object",
  "properties": {
    "backend": { "enum": ["postgres", "s3"] },
    "s3": { "type": "object" }
  }
}
```

- [ ] **Step 4: Verify + commit**

```bash
helm lint deploy/helm/knot --set database.url=x --set session.key=y
helm lint deploy/helm/knot --set database.url=x --set session.key=y --set blob.backend=s3 --set blob.s3.bucket=k
git add deploy/helm/
git commit -m "feat(deploy): blob.backend = postgres | s3 + S3 env wiring"
```

---

## Task 13: Drop image e2e

**Files:**
- Create: `e2e/flows/upload-image.spec.ts`

- [ ] **Step 1: Spec**

```ts
import { execSync } from "node:child_process";
import { expect, test } from "@playwright/test";

function reset() {
  const tables = ["acl_invalidations","audit_events","doc_markdown_cache","doc_snapshots","doc_updates","document_grants","documents","sessions","workspace_members","users","workspaces","blobs","blob_bytes"].join(", ");
  execSync(`docker compose -f deploy/compose/dev.yml exec -T postgres psql -U knot -d knot -c "TRUNCATE TABLE ${tables} CASCADE"`, { cwd: "..", stdio: "pipe" });
}
test.beforeAll(reset);

// 1×1 transparent PNG.
const TINY_PNG = Buffer.from(
  "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d4944415478da636060000000000400015c5b66e30000000049454e44ae426082",
  "hex",
);

test("drop a PNG → renders as <img>, reload preserves", async ({ page }) => {
  await page.goto("/setup");
  await page.getByTestId("setup-email").fill("o@e.com");
  await page.getByTestId("setup-display-name").fill("O");
  await page.getByTestId("setup-password").fill("owner-hunter22");
  await page.getByTestId("setup-submit").click();
  await page.getByTestId("new-doc").click();
  await page.waitForURL(/\/doc\/.+/);
  const url = page.url();
  await expect(page.getByTestId("status-dot")).toHaveAttribute("data-status", "connected", { timeout: 10_000 });

  // Synthesize a drop via DataTransfer (Playwright dispatchEvent).
  await page.evaluate(async (b64) => {
    const file = new File(
      [Uint8Array.from(atob(b64), c => c.charCodeAt(0))],
      "tiny.png",
      { type: "image/png" },
    );
    const dt = new DataTransfer();
    dt.items.add(file);
    const editor = document.querySelector("[data-testid='editor-host'] .ProseMirror")!;
    editor.dispatchEvent(new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: dt }));
  }, TINY_PNG.toString("base64"));

  // Wait for the img to appear.
  const img = page.locator("[data-testid='editor-host'] img");
  await expect(img).toBeVisible({ timeout: 5_000 });
  const src = await img.getAttribute("src");
  expect(src).toMatch(/^\/api\/blobs\//);

  // Reload — Y.Doc persistence should preserve the image node.
  await page.goto(url);
  await expect(page.locator("[data-testid='editor-host'] img")).toBeVisible({ timeout: 5_000 });
});
```

- [ ] **Step 2: Run + commit**

```bash
cd e2e
pnpm playwright test upload-image.spec.ts
git add e2e/
git commit -m "test(e2e): drop a PNG, reload, blob URL persists"
```

If the drop dispatch flaps in headless, fall back to `page.evaluate` that calls `blobsApi.upload` directly and inserts via the editor's API.

---

## Task 14: Outcome doc

Same shape as Plan 12. Status, gates, what landed, what's deferred, carryforward (recommend Plan 14 search next).

```bash
git add docs/
git commit -m "docs: Plan 13 outcome"
```

---

## Self-review checklist

- [ ] `cargo test --workspace` green (blobs integration tests pass)
- [ ] `cargo build --features knot-storage/s3` compiles (manual; not in default CI)
- [ ] `pnpm tsc/lint/test` clean (+1 vitest if anything testable)
- [ ] `pnpm playwright test` — at least 20 passed + the new image spec
- [ ] `helm lint` clean for both backends (postgres + s3)
- [ ] Manual: drop a JPG → `<img>` appears, reload → still there
- [ ] Manual: drop a 12 MB file → toast "File is too large"
- [ ] Manual: invited Editor can upload + viewer can view but not upload
- [ ] Manual: revoking access → image URLs return 403 for the revoked user
