# Runtime-Selected S3 Backend Implementation Plan (Plan 13.5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the S3 blob backend selectable at runtime via env vars instead of at compile time via a cargo feature flag. After this plan: a single binary handles both backends; ops flips `KNOT_BLOB_BACKEND=s3` and supplies AWS creds + bucket to switch. No rebuild, no per-deployment cargo features.

**Architecture:**
- `s3` cargo feature on `knot-storage` is removed. `aws-sdk-s3` becomes a regular `[dependencies]` entry.
- `knot-config::Config` gains `s3_bucket`, `s3_endpoint`, `s3_region`, `s3_prefix`. Empty defaults; figment fills them from `KNOT_*` env.
- `main.rs` reads `KNOT_BLOB_BACKEND`. If `s3`, build `S3Store` from an `aws_config::defaults(...).endpoint_url(...).load()` + the bucket. Otherwise default to `PgBytesStore`.
- AWS credentials follow the standard AWS SDK chain: env (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`), shared profile, IRSA. Helm chart's `blob.s3.existingSecretName` mounts a Secret as `envFrom` so the container picks them up automatically.
- Image grows by ~25–30 MB (aws-sdk-s3 1.x + aws-config). Acceptable cost for the one-binary ops story.

**Predecessor:** Plan 13 (file uploads, HEAD `83e90f1`).

**Tech Stack:** No new deps; existing `aws-sdk-s3` + `aws-config` become non-optional.

**Out of scope:**
- **MinIO sidecar in dev compose.** Out per explicit user preference. The S3 path is exercised by the user's real S3 in production; we don't need a local emulator.
- **S3 integration tests.** Same reason; the trait is uniform, and the unit-level shape is already covered by the build.
- **Per-bucket ACL/IAM helpers.** The chart documents what permissions the bucket needs; ops grants them.
- **Cross-region replication tuning, multipart uploads, presigned URLs.** All future plans.

---

## File map

```
crates/knot-storage/Cargo.toml                          (modify) drop [features], pull AWS deps in unconditionally
crates/knot-storage/src/blobs.rs                        (modify) drop #[cfg] gates on s3 module + re-export
crates/knot-storage/src/lib.rs                          (modify) drop #[cfg] gate on S3Store re-export

crates/knot-config/src/lib.rs                           (modify) +s3_bucket/endpoint/region/prefix fields

crates/knot-server/src/main.rs                          (modify) env-driven backend selection at startup

deploy/helm/knot/values.yaml                            (already has blob.s3.* from Plan 13; ensure existingSecretName documented)
deploy/helm/knot/templates/deployment.yaml              (modify) envFrom blob.s3.existingSecretName when set

docs/superpowers/research/2026-06-03-plan13.5-outcome.md (new)
docs/superpowers/README.md                              (modify) add Plan 13.5 row
```

---

## Task overview

| # | Title | LOC ≈ |
|---|---|---|
| 1 | Drop `s3` cargo feature; deps are unconditional | 30 |
| 2 | Config: +s3_* fields | 40 |
| 3 | `main.rs`: env-driven blob backend selection | 80 |
| 4 | Helm: envFrom existingSecretName for S3 creds | 30 |
| 5 | Outcome doc + README row | 0 |

---

## Task 1: Drop `s3` cargo feature

**Files:**
- `crates/knot-storage/Cargo.toml`
- `crates/knot-storage/src/blobs.rs`
- `crates/knot-storage/src/lib.rs`

- [ ] **Step 1: Cargo.toml**

Remove the `[features]` section's `s3` entry (delete the whole `[features]` block if `default` was the only sibling). Change the `aws-sdk-s3` and `aws-config` deps from `optional = true` to non-optional:

```toml
[dependencies.aws-sdk-s3]
version = "1"
default-features = false
features = ["behavior-version-latest", "rt-tokio", "rustls"]

[dependencies.aws-config]
version = "1"
default-features = false
features = ["behavior-version-latest", "rt-tokio", "rustls"]
```

- [ ] **Step 2: blobs.rs**

Remove the `#[cfg(feature = "s3")]` attributes from the `pub mod s3` declaration and the `pub use s3::S3Store` re-export. They become unconditional.

- [ ] **Step 3: lib.rs**

Remove `#[cfg(feature = "s3")]` from the `pub use blobs::S3Store` line.

- [ ] **Step 4: Verify**

```bash
cargo check -p knot-storage
cargo clippy -p knot-storage --all-targets --all-features -- -D warnings
cargo test -p knot-storage
```

All clean.

- [ ] **Step 5: Commit**

```bash
git add crates/knot-storage/
git commit -m "build(knot-storage): S3 backend is always compiled in (drop cargo feature)"
```

---

## Task 2: Config — S3 fields

**Files:**
- `crates/knot-config/src/lib.rs`

- [ ] **Step 1: Add fields**

In the `Config` struct (alongside the existing `oidc_*` fields), add:

```rust
/// Blob storage backend: "postgres" (default) or "s3".
pub blob_backend: String,
/// S3 bucket (required when blob_backend = s3).
pub s3_bucket: String,
/// S3 endpoint URL (e.g. https://s3.us-east-1.amazonaws.com or
/// http://minio.local:9000 for S3-compatible providers). Leave empty
/// for native AWS S3.
pub s3_endpoint: String,
/// S3 region.
pub s3_region: String,
/// Optional key prefix (e.g. "knot/blobs"). Empty = bucket root.
pub s3_prefix: String,
```

And in `Default for Config`:

```rust
blob_backend: "postgres".into(),
s3_bucket: String::new(),
s3_endpoint: String::new(),
s3_region: "us-east-1".into(),
s3_prefix: String::new(),
```

Also add to the `validate` / `required` list (around line 162) if there is one, so `KNOT_BLOB_BACKEND=s3` requires `s3_bucket`.

- [ ] **Step 2: Verify**

```bash
cargo check -p knot-config
cargo test -p knot-config
```

- [ ] **Step 3: Commit**

```bash
git add crates/knot-config/
git commit -m "feat(knot-config): blob_backend + s3_* fields"
```

---

## Task 3: main.rs — env-driven backend selection

**Files:**
- `crates/knot-server/src/main.rs`

- [ ] **Step 1: Pick backend**

Locate the spot where `AppState::with_pool(pool)` is called and where `state.blob_store`/`state.blob_meta` are populated (right now `with_pool` always sets them to `PgBytesStore`). After `with_pool`, override the blob store when configured:

```rust
if cfg.blob_backend == "s3" {
    if cfg.s3_bucket.is_empty() {
        eprintln!("KNOT_S3_BUCKET is required when KNOT_BLOB_BACKEND=s3");
        process::exit(2);
    }
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new(cfg.s3_region.clone()));
    if !cfg.s3_endpoint.is_empty() {
        loader = loader.endpoint_url(cfg.s3_endpoint.clone());
    }
    let sdk = loader.load().await;
    let mut s3_builder = aws_sdk_s3::config::Builder::from(&sdk);
    if !cfg.s3_endpoint.is_empty() {
        // Path-style addressing for S3-compat providers (MinIO, R2 sub-buckets).
        s3_builder = s3_builder.force_path_style(true);
    }
    let client = aws_sdk_s3::Client::from_conf(s3_builder.build());
    let s3: std::sync::Arc<dyn knot_storage::BlobStore> = std::sync::Arc::new(
        knot_storage::S3Store::new(client, cfg.s3_bucket.clone(), cfg.s3_prefix.clone()),
    );
    s.blob_store = Some(s3);
    tracing::info!(bucket=%cfg.s3_bucket, endpoint=%cfg.s3_endpoint, "blob backend: s3");
} else {
    tracing::info!("blob backend: postgres");
}
```

(Adapt the exact `aws-sdk-s3` and `aws-config` API to the installed version — if `aws_config::defaults` is the wrong name or `BehaviorVersion::latest()` lives elsewhere, fix locally. Don't fight the SDK.)

The `cfg` value is available where the pool is constructed (look at how it's passed in already — it's `Arc<Config>`).

- [ ] **Step 2: Verify**

```bash
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

All clean.

- [ ] **Step 3: Commit**

```bash
git add crates/knot-server/
git commit -m "feat(knot-server): env-driven blob backend selection (postgres | s3)"
```

---

## Task 4: Helm — envFrom existingSecretName

**Files:**
- `deploy/helm/knot/templates/deployment.yaml`
- `deploy/helm/knot/README.md` (modify — document the secret keys)

- [ ] **Step 1: deployment.yaml**

Find the container's `envFrom:` block (it currently has `configMapRef` + `secretRef` for the main session+db secret). Add a conditional second secretRef:

```yaml
{{- if and (eq .Values.blob.backend "s3") .Values.blob.s3.existingSecretName }}
            - secretRef:
                name: {{ .Values.blob.s3.existingSecretName }}
{{- end }}
```

- [ ] **Step 2: README**

Add a short section under "OIDC" or "Using an external Secret":

```markdown
### S3 blob backend

When `blob.backend=s3`, the chart writes `KNOT_BLOB_BACKEND` and the
non-secret S3 config (bucket, endpoint, region, prefix) into the ConfigMap.
AWS credentials should come from a Secret you maintain separately. The
chart mounts that Secret via `envFrom` when `blob.s3.existingSecretName`
is set. Expected keys:

| Key | Purpose |
|-----|---------|
| `AWS_ACCESS_KEY_ID` | static access key (omit for IRSA) |
| `AWS_SECRET_ACCESS_KEY` | static secret (omit for IRSA) |
| `AWS_SESSION_TOKEN` | optional, for STS / IRSA |

EKS users using IRSA can leave `existingSecretName` empty and rely on the
ServiceAccount's IAM role binding instead.
```

- [ ] **Step 3: Verify**

```bash
helm lint deploy/helm/knot \
  --set database.url=x --set session.key=y \
  --set blob.backend=s3 --set blob.s3.bucket=knot-blobs \
  --set blob.s3.existingSecretName=knot-aws-creds
helm template knot deploy/helm/knot \
  --set database.url=x --set session.key=y \
  --set blob.backend=s3 --set blob.s3.bucket=knot-blobs \
  --set blob.s3.existingSecretName=knot-aws-creds \
  | grep -A 3 envFrom
```

- [ ] **Step 4: Commit**

```bash
git add deploy/helm/
git commit -m "feat(deploy): envFrom S3 credentials secret when blob.backend=s3"
```

---

## Task 5: Outcome + index

**Files:**
- `docs/superpowers/research/2026-06-0X-plan13.5-outcome.md`
- `docs/superpowers/README.md`

Use the same shape as prior outcome docs. Status, gates, what landed, what's deferred (image size impact noted), carryforward.

```bash
git add docs/
git commit -m "docs: Plan 13.5 outcome — runtime-selected S3 backend"
```

---

## Self-review checklist

- [ ] `cargo build --release --bin knot-server` builds without `--features`
- [ ] `cargo test --workspace` green
- [ ] `helm lint` clean for both `blob.backend=postgres` and `blob.backend=s3` (with bucket + existingSecretName)
- [ ] `make image.build.host` succeeds; record the new image size in the outcome doc
- [ ] No remaining `#[cfg(feature = "s3")]` attributes anywhere in the workspace
- [ ] `KNOT_BLOB_BACKEND=s3` without `KNOT_S3_BUCKET` exits 2 with a clear error
