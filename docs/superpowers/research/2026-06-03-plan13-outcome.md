# Plan 13 Outcome — File Uploads & Attachments

**Status:** GO. All 14 tasks landed; all gates green.

**Verdict:** knot is now actually useful as a Notion-clone — you can drop a screenshot into a doc and it renders. The `BlobStore` trait abstraction means deployments outgrowing Postgres bytea can swap to S3 by enabling the `s3` cargo feature and setting `blob.backend=s3` in Helm. Recommended next: **Plan 14 (full-text search)** to round out the v0.1 product surface.

## What landed

Plan 13 commits (HEAD `4b286de`):

| Commit | Task | Subject |
|---|---|---|
| b3c05cf | T1 | migrations: blobs + blob_bytes tables (10 MB CHECK) |
| 271f13a | T2-3 | `knot-storage`: BlobStore trait + BlobMeta + PgBytesStore |
| cfd93dc | T4 | `knot-storage`: S3Store behind `s3` feature |
| 20b9aac | T5-7 | `knot-server`: POST/GET/DELETE blob routes |
| 98b60b5 | T8 | server integration tests (7 cases) |
| c82f25e | T9 | `web`: blobsApi + Tiptap Image extension |
| 61cfc28 | T10 | `web`: Attachment Tiptap node (custom) |
| 3310101 | T11 | `web`: editor drop/paste handler |
| 3b7f9d8 | T12 | `deploy`: `blob.backend` + S3 env wiring |
| 4b286de | T13 | e2e: drop a PNG, reload, blob URL persists |

T14 is this outcome doc.

## Gates

- `cargo test --workspace` — green (+7 cases in `blobs_integration.rs`)
- `cargo check -p knot-storage --features s3` — clean (AWS SDK 1.x compiles with `default-features = false` + `behavior-version-latest, rt-tokio, rustls`)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean
- `pnpm tsc/lint/test` — clean
- `pnpm playwright test` — **20 passed, 1 skipped** (the documented `ws-reconnect` spec from Plan 12)
- `pnpm build` — main bundle **111 KB gzipped**, editor chunk **146 KB gzipped**, both under the 250 KB budget despite adding `@tiptap/extension-image` and the custom Attachment node
- `helm lint` — clean for both `blob.backend=postgres` (default) and `blob.backend=s3` (required-value check works)

## Architecture summary

**BlobStore trait** (in `knot-storage`):

```rust
#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, id: Uuid, bytes: &[u8], content_type: &str) -> Result<()>;
    async fn get(&self, id: Uuid) -> Result<Vec<u8>>;
    async fn delete(&self, id: Uuid) -> Result<()>;
}
```

Two implementations:
- `PgBytesStore` — default; stores in `blob_bytes(blob_id PK FK CASCADE, bytes BYTEA)`.
- `S3Store` — `#[cfg(feature = "s3")]`; uses `aws-sdk-s3` 1.x with `default-features = false`. Works with native S3, MinIO, R2. Keys are `<prefix>/<uuid>`.

**Metadata** is stored separately in `blobs` (id, workspace_id, doc_id FK CASCADE, content_type, byte_size BIGINT CHECK 1..=10MB, sha256 BYTEA, original_name, created_by, created_at) — backend-agnostic. The `BlobMeta` struct in `knot-storage` handles all metadata ops.

**Server routes** (`crates/knot-server/src/routes/api/blobs.rs`):
- `POST /api/docs/:doc_id/blobs` — multipart, requires Owner|Editor effective_role on the doc. 10 MB stream limit via `multer::Constraints`. Content-type blocklist on executable prefixes. Inserts metadata first, then bytes (FK CASCADE order). SHA-256 computed server-side. Returns `BlobResponse { id, doc_id, content_type, byte_size, url, original_name }`.
- `GET /api/blobs/:id` — re-runs `effective_role` for the doc on every request (any role suffices). `Cache-Control: private, max-age=60`.
- `DELETE /api/blobs/:id` — Owner|Editor required. Cascade-cleans both tables.

**Frontend:** `blobsApi.upload(docId, file)` uses raw `fetch` (apiFetch doesn't handle FormData), reads the CSRF cookie. The `EditorBody` component receives `docId` as a prop; `editorProps.handleDrop` and `editorProps.handlePaste` route image-MIME files to the Tiptap Image extension's `setImage({ src })` and everything else to the custom Attachment node's `insertContent({ type: "attachment", attrs: { url, name, size, contentType } })`. Errors map to user-friendly toasts (`blob.too_large` → "File too large (10 MB cap)", `blob.blocked_type` → "File type not allowed", `acl.no_grant` → "You don't have permission to upload here").

**Attachment node:** custom `Node` with `addNodeView(ReactNodeViewRenderer)`. Renders as a download link with filename + size badge. `atom: true` (no editable content), `group: "block"`.

**Helm:** `blob.backend = postgres | s3`. When `s3`, requires `blob.s3.bucket` (rendered via Helm `required`). S3 env vars (`KNOT_S3_BUCKET`, `KNOT_S3_ENDPOINT`, `KNOT_S3_REGION`, `KNOT_S3_PREFIX`) are added to the ConfigMap; AWS creds come from `blob.s3.existingSecretName` (deferred to a per-cluster setup — chart documents the expected keys).

## What was non-obvious

**FK insert order matters.** The implementer caught this during integration tests: `blob_bytes.blob_id REFERENCES blobs(id) ON DELETE CASCADE` means the metadata row must exist before the bytes row, otherwise the bytes insert fails with FK violation. The first draft had it backwards. Fixed in the same commit that landed the integration tests.

**`knot-test-support` migration caching.** `sqlx::migrate!("../../migrations")` embeds migration checksums at compile time. When T1 added the blobs migration, the test-support crate's cached artifact didn't include it — the fix was `cargo clean -p knot-test-support` before re-running. Documented inline in the test commit.

**Tiptap 3.x vs 2.x.** pnpm pulled `@tiptap/extension-image` 3.x by default; the rest of the Tiptap ecosystem is 2.x. Downgraded to 2.27.2 to match. The same gotcha hit Plan 7's link extension.

**`useEditor` + drop handler dep cycle.** `editorProps.handleDrop` needs to call `uploadAndInsert`, which calls `editor.chain().setImage(...)`. Including `editor` in the `useEditor` deps array creates a self-referential cycle. The fix: `useRef<Editor>` + `useCallback([docId, notify])`, with the ref assigned synchronously after `useEditor` returns. The drop callback reads `editorRef.current` and stays stable.

**`@tiptap/extension-image` v2 setImage type.** `setImage({ src })` — that's all that's needed. The v3 API requires more attrs.

**Playwright `DragEvent` dispatch works in Chromium.** The naive `dragTo` doesn't trigger ProseMirror's drop, but constructing a `DataTransfer` + `DragEvent` with `clientX`/`clientY` from the editor's `boundingClientRect` and dispatching directly works first try. Documented inline in the e2e spec.

## What's still deferred

- **Image transcoding / thumbnails** — original bytes only. A future CDN setup can resize.
- **Per-blob versioning** — Y.Doc captures the embed reference history; blobs themselves are content-addressed by sha256 (stored but not yet used for dedup).
- **Garbage collection of orphan blobs** — doc archival cascade-deletes via FK. Orphans from copy/paste are a follow-up GC job.
- **Pre-signed S3 URLs for direct browser upload** — would skip the server hop and unlock larger uploads. CORS surface; defer.
- **Antivirus scanning** — out of scope.
- **Per-user / workspace storage quotas** — only the `byte_size` column gives the building block.
- **MinIO sidecar in CI** — S3Store's build is verified but not its runtime behavior. A follow-up plan adds a MinIO container to the dev compose stack + integration tests for the S3 path.
- **`KNOT_BLOB_BACKEND=s3` env routing in `main.rs`.** The Helm chart writes the env var, but `with_pool` always uses the Postgres backend. Wiring `main.rs` to read the env and build the right backend (using `#[cfg(feature = "s3")]`) is a small follow-up — left out so it can land alongside the MinIO sidecar.

## Carryforward for the next plan

1. **Plan 14 — Full-text search.** Postgres FTS over `doc_markdown_cache` for v0.1. Adds a search bar in the command palette + a dedicated search page. Small plan (~6-8 tasks).
2. **Plan 13.5 — S3 runtime wiring + MinIO in dev compose.** Closes the remaining gap from this plan: env-driven backend choice + a MinIO sidecar in `deploy/compose/dev.yml` + integration tests for the S3 path.
3. **Plan 12.5 — Chaos coverage.** WS reconnect via toxiproxy, Postgres restart drill.

## Files of interest

| Path | Role |
|---|---|
| `migrations/20260603091716_blobs.sql` | tables + 10 MB CHECK constraint |
| `crates/knot-storage/src/blobs.rs` | trait + `BlobMeta` |
| `crates/knot-storage/src/blobs/pg.rs` | `PgBytesStore` |
| `crates/knot-storage/src/blobs/s3.rs` | `S3Store` (feature `s3`) |
| `crates/knot-server/src/routes/api/blobs.rs` | upload/download/delete handlers |
| `crates/knot-server/tests/blobs_integration.rs` | 7 ACL + size + content-type cases |
| `web/src/lib/blobs.api.ts` | client upload with FormData + CSRF |
| `web/src/features/editor/extensions.ts` | Image + Attachment registered |
| `web/src/features/editor/nodes/AttachmentNode.tsx` | custom Tiptap node |
| `web/src/features/editor/KnotEditor.tsx` | handleDrop + handlePaste in editorProps |
| `deploy/helm/knot/values.yaml` | `blob.backend` + S3 block |
| `deploy/helm/knot/templates/configmap.yaml` | `KNOT_BLOB_BACKEND` + S3 env conditional |
| `e2e/flows/upload-image.spec.ts` | DataTransfer drop + reload assertion |
