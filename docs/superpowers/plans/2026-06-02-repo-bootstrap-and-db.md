# Repo Bootstrap & DB — Implementation Plan (Plan 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make knot production-shaped before Plans 3-9 add user-facing features — Postgres + migrations, layered config, observability that's usable on day 1, and a CI gate that runs every check the developers actually care about.

**Architecture:** Three new workspace crates (`knot-config`, `knot-obs`, `knot-storage`) so each concern lives in one focused crate with its own tests. `knot-server::main` becomes thin: load config → init observability → connect Postgres → mount HTTP routes. Dev compose runs Postgres locally; tests use `testcontainers` for ephemeral databases. CI runs everything that `make lint test e2e` does, plus `cargo deny`.

**Tech Stack:** PostgreSQL 16 (via Docker compose for dev, testcontainers for tests). `sqlx` 0.8 with compile-time-checked queries + `sqlx migrate`. `figment` 0.10 for layered config. `tracing` + `tracing-subscriber` + `tracing-opentelemetry` + `metrics` + `metrics-exporter-prometheus`. GitHub Actions for CI. `cargo-deny` for supply-chain checks.

**Predecessor:** Plan 1 (Foundation Spike, tag `spike-complete` at `41be127`). Cargo workspace + four crates exist; in-memory only.

**Out of scope for this plan** (each gets its own later plan): auth (Plan 3), workspace/doc tree/ACL (Plan 4), CRDT room actor + persistence (Plan 5), frontend shell beyond the spike (Plans 6-8), Helm chart + release (Plan 9).

---

## Spec coverage map

What this plan implements from `docs/superpowers/specs/2026-06-01-knot-foundation-design.md`:

| Spec section | Tasks |
|---|---|
| §5 Data model — all 11 tables | T4 (single initial migration) |
| §11.1 Dev environment + compose | T1 |
| §11.3 Configuration via `figment` | T2 |
| §11.5 CI per-PR job DAG | T9 + T10 |
| §11.7 Observability — `tracing` + Prom + OTLP | T3, T6 |
| §6.1 Health & meta endpoints (`/api/healthz`, `/api/readyz`, `/api/version`) | T7 |

What this plan deliberately defers to later plans (so they aren't surprises):

- `knot-storage` trait *definitions* in T5 — but only `DocStore` is sketched, the others (`SessionStore`, `BlobStore`) land in Plan 3/Plan 5 when they have first consumers.
- No data is actually written to any table in Plan 2 — the schema lands and is tested against fresh DBs only. Plans 3-5 add the queries.
- The Markdown cache and audit log get schemas but no UI / endpoints.

---

## File map

```
knot/
├── Cargo.toml                          (modify) add new members + new workspace deps
│
├── crates/
│   ├── knot-config/                    (new) figment-based config crate
│   │   ├── Cargo.toml
│   │   ├── src/lib.rs                  Config struct + load() + tests
│   │   └── tests/load.rs               layered config integration tests
│   │
│   ├── knot-obs/                       (new) observability primitives
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  re-exports
│   │       ├── logging.rs              tracing-subscriber setup
│   │       ├── metrics.rs              prometheus exporter + counters
│   │       └── tracing.rs              OTLP exporter setup
│   │
│   ├── knot-storage/                   (new) sqlx + storage traits
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs                  re-exports
│   │   │   ├── pool.rs                 PgPool factory
│   │   │   └── doc_store.rs            DocStore trait + sqlx impl stub
│   │   └── tests/
│   │       └── migrations_apply.rs     testcontainers smoke
│   │
│   └── knot-server/                    (modify) wire new crates into main
│       ├── Cargo.toml                  (modify) +knot-config, +knot-obs, +knot-storage
│       └── src/
│           ├── main.rs                 (rewrite) load config → init obs → connect DB → serve
│           ├── lib.rs                  (modify) AppState gets PgPool
│           └── routes/
│               ├── mod.rs              (new) router composition
│               ├── health.rs           (new) /api/healthz + /api/readyz + /api/version
│               └── collab.rs           (move) extracted from src/lib.rs
│
├── migrations/                         (new) sqlx migrations
│   └── 20260602000001_v0_1_schema.sql  all 11 tables in one initial migration
│
├── deploy/
│   └── compose/
│       └── dev.yml                     (new) Postgres 16 + healthcheck
│
├── Makefile                            (modify) +compose.* +migrate.* targets
│
├── .github/
│   └── workflows/
│       └── ci.yml                      (new) fmt + clippy + test + e2e + build + deny
│
├── deny.toml                           (new) cargo-deny config
│
└── docs/superpowers/
    ├── plans/2026-06-02-repo-bootstrap-and-db.md   (this file)
    └── research/
        └── 2026-06-02-plan2-outcome.md (new, written in T11)
```

---

## Conventions for this plan

- **Every code task is TDD.** Failing test → minimal impl → green test → commit.
- **Tests run with `cargo nextest`.** Integration tests use the `integration` feature flag (so `make test.rust` runs them by default in this project; CI runs `--features integration` explicitly to be explicit).
- **`testcontainers-modules`** is the canonical Postgres-in-tests dependency. We use the `postgres` feature.
- **`sqlx::query!`** (compile-time-checked) is preferred where possible. We accept that this means `cargo build` needs a live DB OR a `.sqlx/` offline cache. We commit the offline cache and document the regen workflow.
- **API drift notice:** `tracing-opentelemetry`, `opentelemetry-otlp`, `sqlx`, and `testcontainers-modules` have churned versions through 2025-2026. Implementers will likely need to follow rustc errors and the crate's current docs for exact method names. The conceptual shape (subscribers, exporters, pools, fixtures) is stable.

---

## Task 1: Postgres dev compose + Makefile targets

**Files:**
- Create: `deploy/compose/dev.yml`
- Modify: `Makefile`

- [ ] **Step 1: Create `deploy/compose/dev.yml`**

```yaml
# Dev compose: Postgres only for now.
# Dex (OIDC) lands in Plan 3; MinIO (blob store) lands when attachments arrive.

services:
  postgres:
    image: postgres:16-alpine
    container_name: knot-dev-postgres
    environment:
      POSTGRES_DB: knot
      POSTGRES_USER: knot
      POSTGRES_PASSWORD: knot
    ports:
      - "5432:5432"
    volumes:
      - knot-dev-postgres:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U knot -d knot"]
      interval: 2s
      timeout: 5s
      retries: 30

volumes:
  knot-dev-postgres:
```

The credentials are hard-coded for dev only — production goes through the real config (T2). Listening on `:5432` localhost-only because Docker maps to `127.0.0.1:5432` by default; we do not bind to all interfaces.

- [ ] **Step 2: Add compose + migrate targets to `Makefile`**

Append after the existing targets (preserve everything above):

```makefile
.PHONY: compose.up
compose.up: ## start dev compose (Postgres) in background
	docker compose -f deploy/compose/dev.yml up -d
	@echo "waiting for Postgres to be healthy..."
	@for i in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do \
		if docker compose -f deploy/compose/dev.yml ps postgres | grep -q "healthy"; then \
			echo "Postgres healthy"; exit 0; \
		fi; sleep 1; \
	done; \
	echo "Postgres did not become healthy in 15s"; exit 1

.PHONY: compose.down
compose.down: ## stop dev compose
	docker compose -f deploy/compose/dev.yml down

.PHONY: compose.logs
compose.logs: ## tail dev compose logs
	docker compose -f deploy/compose/dev.yml logs -f

.PHONY: compose.psql
compose.psql: ## psql into the dev Postgres
	docker compose -f deploy/compose/dev.yml exec postgres psql -U knot -d knot

.PHONY: migrate.up
migrate.up: ## apply pending migrations (against $$DATABASE_URL or compose default)
	DATABASE_URL=$${DATABASE_URL:-postgres://knot:knot@localhost:5432/knot} \
		sqlx migrate run --source migrations

.PHONY: migrate.down
migrate.down: ## revert the most recent migration
	DATABASE_URL=$${DATABASE_URL:-postgres://knot:knot@localhost:5432/knot} \
		sqlx migrate revert --source migrations

.PHONY: migrate.info
migrate.info: ## show migration status
	DATABASE_URL=$${DATABASE_URL:-postgres://knot:knot@localhost:5432/knot} \
		sqlx migrate info --source migrations
```

- [ ] **Step 3: Smoke test the compose stack**

```
make compose.up
docker compose -f deploy/compose/dev.yml exec postgres pg_isready -U knot -d knot
make compose.down
```

Expected: `make compose.up` reports "Postgres healthy" within 15s; `pg_isready` returns `accepting connections`; `make compose.down` cleans up.

- [ ] **Step 4: Commit**

```
git add deploy/compose Makefile
git commit -m "feat(dev): postgres compose stack + Makefile targets"
```

---

## Task 2: `knot-config` crate — layered config via figment

**Files:**
- Modify: `Cargo.toml` (root workspace — add `knot-config` member + `figment` workspace dep)
- Create: `crates/knot-config/Cargo.toml`
- Create: `crates/knot-config/src/lib.rs`
- Create: `crates/knot-config/tests/load.rs`

- [ ] **Step 1: Workspace + crate manifest**

In `/home/nik/Development/knot/Cargo.toml`:

Add to `[workspace.dependencies]`:

```toml
figment = { version = "0.10", features = ["env", "toml", "yaml"] }
```

Update `[workspace] members`:

```toml
members = [
    "tools/schemagen",
    "crates/knot-crdt",
    "crates/knot-markdown",
    "crates/knot-config",
    "crates/knot-server",
]
```

Create `crates/knot-config/Cargo.toml`:

```toml
[package]
name = "knot-config"
version = "0.0.0"
edition.workspace = true

[dependencies]
serde.workspace = true
figment.workspace = true
thiserror.workspace = true
```

- [ ] **Step 2: Write the failing test**

`crates/knot-config/tests/load.rs`:

```rust
//! Integration tests for the layered config loader.
//!
//! Verifies: defaults < file < env < (process-set) ordering and that
//! KNOT_* env vars override file values.

use knot_config::Config;

#[test]
fn defaults_when_no_env_no_file() {
    figment::Jail::expect_with(|_jail| {
        let cfg = Config::load(None).expect("load");
        assert_eq!(cfg.addr, ":3000");
        assert_eq!(cfg.env, "development");
        assert!(cfg.database_url.is_empty(), "database_url empty by default");
        assert!(!cfg.tracing_enabled);
        assert_eq!(cfg.log_level, "info");
        Ok(())
    });
}

#[test]
fn env_overrides_defaults() {
    figment::Jail::expect_with(|jail| {
        jail.set_env("KNOT_ADDR", ":9999");
        jail.set_env("KNOT_DATABASE_URL", "postgres://x:y@h/d");
        jail.set_env("KNOT_LOG_LEVEL", "debug");
        let cfg = Config::load(None).expect("load");
        assert_eq!(cfg.addr, ":9999");
        assert_eq!(cfg.database_url, "postgres://x:y@h/d");
        assert_eq!(cfg.log_level, "debug");
        Ok(())
    });
}

#[test]
fn file_overrides_defaults_env_overrides_file() {
    figment::Jail::expect_with(|jail| {
        jail.create_file(
            "config.yaml",
            r#"
addr: ":7777"
log_level: warn
database_url: postgres://file:host/db
"#,
        )?;
        // No env yet: file values win.
        let cfg = Config::load(Some("config.yaml")).expect("load");
        assert_eq!(cfg.addr, ":7777");
        assert_eq!(cfg.log_level, "warn");

        // Set env: it overrides the file.
        jail.set_env("KNOT_ADDR", ":8888");
        let cfg = Config::load(Some("config.yaml")).expect("load with env");
        assert_eq!(cfg.addr, ":8888");
        assert_eq!(cfg.log_level, "warn", "log_level still from file");
        Ok(())
    });
}

#[test]
fn refuses_empty_session_key_in_production() {
    figment::Jail::expect_with(|jail| {
        jail.set_env("KNOT_ENV", "production");
        let result = Config::load(None);
        assert!(
            result.is_err(),
            "production with no session key must fail to load"
        );
        Ok(())
    });
}
```

- [ ] **Step 3: Verify FAIL**

```
cd /home/nik/Development/knot && cargo nextest run -p knot-config
```

Expected: compile error (no `knot_config::Config`).

- [ ] **Step 4: Implement `crates/knot-config/src/lib.rs`**

```rust
//! Layered configuration loader.
//!
//! Precedence (lowest → highest): defaults < optional file < environment.
//! Environment variables are prefixed `KNOT_` and lowercased to match
//! the field names (e.g. `KNOT_ADDR` → `addr`).

use std::path::Path;

use figment::{
    providers::{Env, Format, Serialized, Yaml},
    Figment,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("figment: {0}")]
    Figment(#[from] figment::Error),
    #[error("invalid: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Listen address for HTTP/WS (e.g. ":3000" or "127.0.0.1:3000").
    pub addr: String,
    /// "development" or "production". Affects strict-mode checks.
    pub env: String,
    /// External base URL (used for OIDC redirect URLs, links, etc.).
    pub base_url: String,
    /// Postgres connection string.
    pub database_url: String,
    /// HMAC key for CSRF token signing. Required in production.
    pub session_key: String,
    /// Filesystem path for blob storage (fs BlobStore impl).
    pub data_dir: String,

    /// Log level for the application: trace/debug/info/warn/error.
    pub log_level: String,
    /// Log format: "json" or "text".
    pub log_format: String,
    /// Listen address for the metrics + pprof endpoints.
    pub metrics_addr: String,
    /// Enable OpenTelemetry OTLP exporter.
    pub tracing_enabled: bool,
    /// OTLP endpoint when tracing is enabled.
    pub otlp_endpoint: String,
    /// Enable pprof endpoints on the metrics port.
    pub pprof_enabled: bool,

    /// CRDT snapshot trigger: N updates between snapshots.
    pub snapshot_every_n: u32,
    /// CRDT snapshot trigger: idle seconds before snapshotting.
    pub snapshot_idle_sec: u32,
    /// CRDT room eviction: idle seconds before unloading a room.
    pub room_idle_evict_sec: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: ":3000".into(),
            env: "development".into(),
            base_url: "http://localhost:3000".into(),
            database_url: String::new(),
            session_key: String::new(),
            data_dir: "./data".into(),
            log_level: "info".into(),
            log_format: "json".into(),
            metrics_addr: ":9090".into(),
            tracing_enabled: false,
            otlp_endpoint: String::new(),
            pprof_enabled: false,
            snapshot_every_n: 200,
            snapshot_idle_sec: 30,
            room_idle_evict_sec: 300,
        }
    }
}

impl Config {
    /// Load configuration with optional yaml file path.
    ///
    /// Precedence: defaults < file (if Some) < env (`KNOT_*`).
    pub fn load(file: Option<impl AsRef<Path>>) -> Result<Self, ConfigError> {
        let mut fig = Figment::from(Serialized::defaults(Config::default()));
        if let Some(path) = file {
            fig = fig.merge(Yaml::file(path));
        }
        let cfg: Config = fig
            .merge(Env::prefixed("KNOT_").split("_"))
            .extract()?;

        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if self.env == "production" && self.session_key.is_empty() {
            return Err(ConfigError::Invalid(
                "KNOT_SESSION_KEY is required when KNOT_ENV=production".into(),
            ));
        }
        if !matches!(
            self.log_level.as_str(),
            "trace" | "debug" | "info" | "warn" | "error"
        ) {
            return Err(ConfigError::Invalid(format!(
                "invalid log_level: {}",
                self.log_level
            )));
        }
        if !matches!(self.log_format.as_str(), "json" | "text") {
            return Err(ConfigError::Invalid(format!(
                "invalid log_format: {}",
                self.log_format
            )));
        }
        Ok(())
    }
}
```

Note on `Env::prefixed("KNOT_").split("_")`: figment's `.split` converts e.g. `KNOT_DATABASE_URL` → `database.url`. **We want flat field names**, so this is wrong for our case. Remove `.split("_")`:

```rust
.merge(Env::prefixed("KNOT_"))
```

This makes `KNOT_DATABASE_URL` map to a top-level `database_url` field via figment's default (case-insensitive, underscore-preserving) behaviour. Verify by reading figment docs if `tests/load.rs` fails — the exact env-mapping rules in figment 0.10 have changed across versions. If the simple form doesn't pick up multi-underscore vars correctly, fall back to setting `Env::prefixed("KNOT_").only(&["addr", "database_url", ...])` enumerating fields.

- [ ] **Step 5: Verify PASS**

```
cargo nextest run -p knot-config
```

Expected: 4/4 tests PASS. Adjust the env loader if any test fails.

- [ ] **Step 6: Lint + commit**

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/knot-config
git commit -m "feat(knot-config): figment-based layered config loader"
```

---

## Task 3: `knot-obs` crate — tracing + metrics + OTLP

**Files:**
- Modify: `Cargo.toml` workspace deps + members
- Create: `crates/knot-obs/Cargo.toml`
- Create: `crates/knot-obs/src/lib.rs`
- Create: `crates/knot-obs/src/logging.rs`
- Create: `crates/knot-obs/src/metrics.rs`
- Create: `crates/knot-obs/src/tracing.rs`

- [ ] **Step 1: Workspace + crate manifest**

Add to `Cargo.toml [workspace.dependencies]`:

```toml
metrics = "0.24"
metrics-exporter-prometheus = "0.16"
opentelemetry = "0.27"
opentelemetry_sdk = { version = "0.27", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.27", features = ["grpc-tonic"] }
tracing-opentelemetry = "0.28"
```

Add `crates/knot-obs` to `members`.

Create `crates/knot-obs/Cargo.toml`:

```toml
[package]
name = "knot-obs"
version = "0.0.0"
edition.workspace = true

[dependencies]
tracing.workspace = true
tracing-subscriber.workspace = true
tracing-opentelemetry.workspace = true
opentelemetry.workspace = true
opentelemetry_sdk.workspace = true
opentelemetry-otlp.workspace = true
metrics.workspace = true
metrics-exporter-prometheus.workspace = true
thiserror.workspace = true
anyhow.workspace = true
tokio.workspace = true
```

- [ ] **Step 2: `crates/knot-obs/src/lib.rs`**

```rust
//! Observability primitives for knot.
//!
//! - `logging::init` sets up `tracing-subscriber` with JSON or text output.
//! - `metrics::init` exposes Prometheus on a configurable port.
//! - `tracing::init_otlp` (optional) attaches an OpenTelemetry OTLP exporter.
//!
//! Modules are independent; a server can opt into any subset.

pub mod logging;
pub mod metrics;
pub mod tracing;

/// Returned by init functions; dropping this triggers shutdown of any
/// background tasks (OTLP batch exporter, prometheus exporter).
pub struct ObsGuard {
    // Empty for now; tracing module populates its tracer-provider here
    // so that a flush happens on Drop.
    _opaque: (),
}

impl ObsGuard {
    pub(crate) fn empty() -> Self {
        Self { _opaque: () }
    }
}
```

- [ ] **Step 3: `crates/knot-obs/src/logging.rs`**

```rust
//! Tracing-subscriber initialisation.
//!
//! Two formats: "json" (production) and "text" (dev). Level filter comes
//! from `RUST_LOG`-style env filter, defaulting to whatever the caller
//! passes (typically `cfg.log_level`).

use std::str::FromStr;

use tracing::Level;
use tracing_subscriber::{filter::EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    #[error("invalid log level: {0}")]
    Level(String),
    #[error("subscriber already initialised")]
    AlreadyInit,
}

/// Initialise the global tracing subscriber.
///
/// `level` is one of "trace"/"debug"/"info"/"warn"/"error".
/// `format` is "json" or "text".
///
/// Honours `RUST_LOG` if set (overrides the level argument); otherwise
/// uses the passed level as the floor.
pub fn init(level: &str, format: &str) -> Result<(), LoggingError> {
    let lvl = Level::from_str(level).map_err(|_| LoggingError::Level(level.into()))?;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(lvl.to_string()));

    let registry = tracing_subscriber::registry().with(filter);
    match format {
        "json" => registry.with(fmt::layer().json()).try_init(),
        _ => registry.with(fmt::layer()).try_init(),
    }
    .map_err(|_| LoggingError::AlreadyInit)?;
    Ok(())
}
```

- [ ] **Step 4: `crates/knot-obs/src/metrics.rs`**

```rust
//! Prometheus metrics exporter.
//!
//! Exposes `/metrics` on a dedicated address (kept separate from the
//! main HTTP server so unauth scrapes don't traverse auth middleware).

use std::net::SocketAddr;

use metrics_exporter_prometheus::PrometheusBuilder;

#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("invalid address: {0}")]
    Address(String),
    #[error("install exporter: {0}")]
    Install(String),
}

/// Install the global metrics recorder and start the HTTP exporter
/// on `addr` (e.g. ":9090" or "0.0.0.0:9090").
pub fn init(addr: &str) -> Result<(), MetricsError> {
    let sa: SocketAddr = normalize_addr(addr)?
        .parse()
        .map_err(|e| MetricsError::Address(format!("{addr}: {e}")))?;

    PrometheusBuilder::new()
        .with_http_listener(sa)
        .install()
        .map_err(|e| MetricsError::Install(e.to_string()))?;
    Ok(())
}

fn normalize_addr(addr: &str) -> Result<String, MetricsError> {
    // Allow ":9090" shorthand by prefixing with 0.0.0.0.
    if let Some(port) = addr.strip_prefix(':') {
        if port.parse::<u16>().is_ok() {
            return Ok(format!("0.0.0.0:{port}"));
        }
    }
    Ok(addr.to_string())
}

#[cfg(test)]
mod tests {
    use super::normalize_addr;

    #[test]
    fn shorthand_port() {
        assert_eq!(normalize_addr(":9090").unwrap(), "0.0.0.0:9090");
    }

    #[test]
    fn explicit_addr() {
        assert_eq!(
            normalize_addr("127.0.0.1:9090").unwrap(),
            "127.0.0.1:9090"
        );
    }
}
```

- [ ] **Step 5: `crates/knot-obs/src/tracing.rs`**

```rust
//! OpenTelemetry OTLP exporter setup.
//!
//! Only used when `KNOT_TRACING_ENABLED=true`. Attaches an OTLP gRPC
//! exporter as a `tracing_opentelemetry` layer over the existing
//! `tracing-subscriber` registry.
//!
//! NOTE: the opentelemetry crate's API has had repeated breaking
//! changes through 2024-2026. If the imports below don't compile
//! against the resolved version of `opentelemetry` / `opentelemetry-otlp`,
//! follow the crate's current `Tracer` builder docs. The conceptual
//! shape (build pipeline, install global, return guard) is stable.

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

#[derive(Debug, thiserror::Error)]
pub enum TracingError {
    #[error("otlp: {0}")]
    Otlp(String),
    #[error("subscriber: {0}")]
    Subscriber(String),
}

/// Initialise the global tracing subscriber WITH an OTLP layer.
///
/// Call this INSTEAD of `logging::init` when OTLP is enabled.
/// The endpoint is typically "http://otel-collector:4317".
pub fn init_with_otlp(
    level: &str,
    format: &str,
    endpoint: &str,
    service_name: &str,
) -> Result<(), TracingError> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .map_err(|e| TracingError::Otlp(e.to_string()))?;

    let provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(opentelemetry_sdk::Resource::new(vec![
            opentelemetry::KeyValue::new("service.name", service_name.to_string()),
        ]))
        .build();
    let tracer = provider.tracer(service_name.to_string());
    opentelemetry::global::set_tracer_provider(provider);

    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(otel_layer);
    match format {
        "json" => registry.with(tracing_subscriber::fmt::layer().json()).try_init(),
        _ => registry.with(tracing_subscriber::fmt::layer()).try_init(),
    }
    .map_err(|e| TracingError::Subscriber(format!("{e}")))?;
    Ok(())
}

/// Shut down the OpenTelemetry exporter — flushes any in-flight spans.
pub fn shutdown() {
    opentelemetry::global::shutdown_tracer_provider();
}
```

The exact builder method names in `opentelemetry_otlp` 0.27 may differ. Likely alternatives:
- `SpanExporterBuilder::new_tonic()` instead of `SpanExporter::builder().with_tonic()`
- `Tonic::new_exporter()` etc.

Follow the rustc errors and the [opentelemetry-otlp examples](https://docs.rs/opentelemetry-otlp/latest/opentelemetry_otlp/) for the resolved version.

- [ ] **Step 6: Run tests**

```
cargo nextest run -p knot-obs
```

Expected: PASS (the `normalize_addr` unit tests run; nothing more — `init` functions install global state and aren't unit-tested).

- [ ] **Step 7: Lint + commit**

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/knot-obs
git commit -m "feat(knot-obs): tracing + metrics + OTLP setup helpers"
```

---

## Task 4: Initial schema migration

**Files:**
- Create: `migrations/20260602000001_v0_1_schema.sql`

We land all 11 v0.1 tables in one migration. Subsequent plans can add migrations as features need schema changes.

- [ ] **Step 1: Create the migration**

`migrations/20260602000001_v0_1_schema.sql`:

```sql
-- v0.1 schema. See Foundation spec §5 for design rationale.
--
-- This migration creates all tables defined in v0.1. Subsequent plans
-- add migrations for schema changes. We do NOT split this into multiple
-- migration files because v0.1 is one canonical schema, not an
-- evolution history.

-- Extensions
CREATE EXTENSION IF NOT EXISTS citext;
CREATE EXTENSION IF NOT EXISTS pgcrypto;  -- for gen_random_uuid()

-- ---------------------------------------------------------------------
-- 5.1 Identity & tenancy
-- ---------------------------------------------------------------------

CREATE TABLE workspaces (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    slug       text UNIQUE NOT NULL,
    name       text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    email         citext UNIQUE NOT NULL,
    display_name  text NOT NULL,
    password_hash text NULL,                 -- NULL for OIDC-only users
    oidc_subject  text NULL,
    oidc_issuer   text NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (oidc_issuer, oidc_subject)
);

CREATE TABLE workspace_members (
    workspace_id uuid REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id      uuid REFERENCES users(id) ON DELETE CASCADE,
    role         text NOT NULL CHECK (role IN ('owner','editor','viewer')),
    added_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id)
);

CREATE TABLE sessions (
    id           bytea PRIMARY KEY,           -- 32 random bytes
    user_id      uuid REFERENCES users(id) ON DELETE CASCADE,
    workspace_id uuid REFERENCES workspaces(id) ON DELETE CASCADE,
    created_at   timestamptz NOT NULL DEFAULT now(),
    expires_at   timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    user_agent   text,
    ip           inet
);
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);

-- ---------------------------------------------------------------------
-- 5.2 Document tree
-- ---------------------------------------------------------------------

CREATE TABLE documents (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    parent_id    uuid NULL REFERENCES documents(id) ON DELETE CASCADE,
    title        text NOT NULL DEFAULT 'Untitled',
    sort_key     text NOT NULL,
    icon         text NULL,
    created_by   uuid NOT NULL REFERENCES users(id),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    archived_at  timestamptz NULL,
    UNIQUE (workspace_id, parent_id, sort_key)
);
CREATE INDEX documents_tree_idx ON documents (workspace_id, parent_id, sort_key);
CREATE INDEX documents_workspace_alive_idx ON documents (workspace_id) WHERE archived_at IS NULL;

-- ---------------------------------------------------------------------
-- 5.3 ACL inheritance
-- ---------------------------------------------------------------------

CREATE TABLE document_grants (
    doc_id     uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    principal  text NOT NULL,        -- 'user:<uuid>' or 'group:<oidc-group>'
    role       text NOT NULL CHECK (role IN ('viewer','editor','owner')),
    inherit    boolean NOT NULL DEFAULT true,
    granted_at timestamptz NOT NULL DEFAULT now(),
    granted_by uuid REFERENCES users(id),
    PRIMARY KEY (doc_id, principal)
);

-- ---------------------------------------------------------------------
-- 5.4 CRDT storage
-- ---------------------------------------------------------------------

CREATE TABLE doc_updates (
    seq          bigserial PRIMARY KEY,
    doc_id       uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    update_bytes bytea NOT NULL,
    by_user_id   uuid NULL REFERENCES users(id),
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX doc_updates_by_doc_idx ON doc_updates (doc_id, seq);

CREATE TABLE doc_snapshots (
    doc_id       uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    snapshot_seq bigint NOT NULL,
    state_bytes  bytea NOT NULL,
    state_vector bytea NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (doc_id, snapshot_seq)
);

-- ---------------------------------------------------------------------
-- 5.5 Markdown cache
-- ---------------------------------------------------------------------

CREATE TABLE doc_markdown_cache (
    doc_id          uuid PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
    rendered_at_seq bigint NOT NULL,
    markdown_text   text NOT NULL,
    updated_at      timestamptz NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------
-- 5.6 Audit / activity (skeleton; no UI in v0.1)
-- ---------------------------------------------------------------------

CREATE TABLE audit_events (
    id           bigserial PRIMARY KEY,
    workspace_id uuid NOT NULL,
    actor_id     uuid NULL,
    action       text NOT NULL,
    target_kind  text NOT NULL,
    target_id    uuid NOT NULL,
    data         jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at   timestamptz NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------
-- 5.7 ACL invalidation outbox
-- ---------------------------------------------------------------------

CREATE TABLE acl_invalidations (
    id           bigserial PRIMARY KEY,
    workspace_id uuid NOT NULL,
    doc_id       uuid NOT NULL,
    reason       text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);
```

- [ ] **Step 2: Apply the migration against the dev DB**

```
make compose.up
make migrate.up
make migrate.info
```

Expected: `migrate.info` shows one applied migration with the timestamp `20260602000001`.

- [ ] **Step 3: Verify the schema**

```
docker compose -f deploy/compose/dev.yml exec postgres \
    psql -U knot -d knot -c "\dt"
```

Expected: list of 11 tables (workspaces, users, workspace_members, sessions, documents, document_grants, doc_updates, doc_snapshots, doc_markdown_cache, audit_events, acl_invalidations) plus the `_sqlx_migrations` bookkeeping table.

- [ ] **Step 4: Revert and re-apply (sanity check)**

```
make migrate.down
make migrate.info
make migrate.up
```

Expected: `migrate.down` removes the migration; `migrate.up` re-applies cleanly.

Note: sqlx migrations don't support `down` SQL by default — `migrate.down` just removes the migration row. The schema doesn't get rolled back automatically. For v0.1 this is fine because there's only the initial migration. If we need real rollbacks later, switch to reversible migrations.

- [ ] **Step 5: Stop compose, commit**

```
make compose.down
git add migrations
git commit -m "feat(db): initial v0.1 schema migration (all 11 tables)"
```

---

## Task 5: `knot-storage` crate — Postgres pool + migrations smoke

**Files:**
- Modify: workspace `Cargo.toml`
- Create: `crates/knot-storage/Cargo.toml`
- Create: `crates/knot-storage/src/lib.rs`
- Create: `crates/knot-storage/src/pool.rs`
- Create: `crates/knot-storage/src/doc_store.rs`
- Create: `crates/knot-storage/tests/migrations_apply.rs`
- Create: `.sqlx/` directory (committed for offline builds) — populated by step 5

- [ ] **Step 1: Workspace + crate manifest**

Add to `Cargo.toml [workspace.dependencies]`:

```toml
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio-rustls", "postgres", "uuid", "chrono", "json", "migrate", "macros"] }
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["postgres"] }
chrono = { version = "0.4", features = ["serde"] }
```

Add `crates/knot-storage` to members.

Create `crates/knot-storage/Cargo.toml`:

```toml
[package]
name = "knot-storage"
version = "0.0.0"
edition.workspace = true

[dependencies]
sqlx.workspace = true
tokio.workspace = true
tracing.workspace = true
thiserror.workspace = true
uuid.workspace = true
chrono.workspace = true

[dev-dependencies]
testcontainers.workspace = true
testcontainers-modules.workspace = true
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

- [ ] **Step 2: `crates/knot-storage/src/lib.rs`**

```rust
//! Storage layer for knot — Postgres pool + storage traits.
//!
//! In v0.1, this crate exposes:
//! - `Pool` (re-export of `sqlx::PgPool`)
//! - `pool::connect` — opens a pool and runs pending migrations
//! - `DocStore` trait — placeholder; implemented in Plan 5
//!
//! Trait definitions for `SessionStore` and `BlobStore` land in their
//! respective plans (3 and later) so the surface area grows organically.

pub mod doc_store;
pub mod pool;

pub use doc_store::DocStore;
pub use pool::{connect, Pool, PoolError};
```

- [ ] **Step 3: `crates/knot-storage/src/pool.rs`**

```rust
//! Postgres connection pool + migration runner.

use std::time::Duration;

use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};
use thiserror::Error;

pub type Pool = PgPool;

#[derive(Debug, Error)]
pub enum PoolError {
    #[error("invalid connection string: {0}")]
    Url(String),
    #[error("connect: {0}")]
    Connect(#[from] sqlx::Error),
    #[error("migrate: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

/// Open a Postgres pool and run pending migrations.
///
/// `max_conn` should typically be `2 * num_cpus` for OLTP workloads;
/// higher for I/O-bound workloads. Plan 5 (room actor) will revisit
/// this once we know how much pool pressure CRDT persistence creates.
pub async fn connect(url: &str, max_conn: u32) -> Result<Pool, PoolError> {
    let opts: PgConnectOptions = url
        .parse()
        .map_err(|e: sqlx::Error| PoolError::Url(e.to_string()))?;

    let pool = PgPoolOptions::new()
        .max_connections(max_conn)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(opts)
        .await?;

    sqlx::migrate!("../../migrations").run(&pool).await?;

    Ok(pool)
}
```

The `sqlx::migrate!` macro reads migrations at compile time from the path **relative to the crate's Cargo.toml**. From `crates/knot-storage/`, the migrations are at `../../migrations`.

- [ ] **Step 4: `crates/knot-storage/src/doc_store.rs`**

```rust
//! Document storage trait — placeholder in v0.1.
//!
//! The real implementation lands in Plan 5 (CRDT room actor + persistence).
//! For Plan 2 we just define the trait shape so consumers can be wired
//! against an interface from day 1.

use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DocStoreError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("not found")]
    NotFound,
}

/// Persistence operations for documents and their CRDT updates.
///
/// Plan 5 will flesh this out with `append_update`, `load_snapshot`,
/// `write_snapshot`, etc. For now we only declare a marker.
#[async_trait]
pub trait DocStore: Send + Sync + 'static {
    /// Returns true if a document with this id exists (and is not archived).
    async fn exists(&self, doc_id: Uuid) -> Result<bool, DocStoreError>;
}
```

Add `async-trait` to workspace deps + this crate's deps:

```toml
# Cargo.toml [workspace.dependencies]
async-trait = "0.1"

# crates/knot-storage/Cargo.toml [dependencies]
async-trait.workspace = true
```

- [ ] **Step 5: Write the migration-applies integration test**

`crates/knot-storage/tests/migrations_apply.rs`:

```rust
//! Verify the v0.1 migration applies cleanly against a fresh Postgres
//! and creates the expected 11 user tables.
//!
//! Uses testcontainers — no need for compose to be running.

use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

#[tokio::test(flavor = "multi_thread")]
async fn migrations_apply_cleanly() {
    let pg = Postgres::default().start().await.expect("start postgres");
    let port = pg.get_host_port_ipv4(5432).await.expect("port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");

    let pool = knot_storage::connect(&url, 4).await.expect("connect + migrate");

    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT table_name::text \
         FROM information_schema.tables \
         WHERE table_schema = 'public' AND table_name != '_sqlx_migrations' \
         ORDER BY table_name",
    )
    .fetch_all(&pool)
    .await
    .expect("query tables");
    let names: Vec<String> = rows.into_iter().map(|(n,)| n).collect();

    let expected: &[&str] = &[
        "acl_invalidations",
        "audit_events",
        "doc_markdown_cache",
        "doc_snapshots",
        "doc_updates",
        "document_grants",
        "documents",
        "sessions",
        "users",
        "workspace_members",
        "workspaces",
    ];
    assert_eq!(
        names.iter().map(String::as_str).collect::<Vec<_>>(),
        expected,
        "v0.1 schema must define exactly these tables"
    );
}
```

- [ ] **Step 6: Run, expect PASS**

```
cd /home/nik/Development/knot && cargo nextest run -p knot-storage
```

Expected: `migrations_apply_cleanly` PASSes (takes ~10-30s for testcontainers to pull and start Postgres first time, ~5s after that).

If `testcontainers-modules` 0.11 has API drift, follow rustc errors:
- `Postgres::default()` may be `Postgres::new()` or need an explicit image tag.
- `start()` vs `start_async()` — pick whichever the macro `#[tokio::test]` accepts.
- `get_host_port_ipv4` vs `get_host_port` — depends on the testcontainers version.

If Docker isn't available, the test will fail with a clear error message. That's acceptable — implementers can run `make compose.up` first to verify Docker works.

- [ ] **Step 7: Lint + commit**

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add Cargo.toml Cargo.lock crates/knot-storage
git commit -m "feat(knot-storage): Postgres pool + migration runner + testcontainers smoke"
```

---

## Task 6: Wire config + obs + storage into `knot-server`

**Files:**
- Modify: `crates/knot-server/Cargo.toml`
- Modify: `crates/knot-server/src/main.rs`
- Modify: `crates/knot-server/src/lib.rs`

This task makes `main.rs` the thin orchestrator the spec calls for: load config → init observability → connect Postgres → mount routes.

- [ ] **Step 1: Update `crates/knot-server/Cargo.toml` dependencies**

Add to `[dependencies]`:

```toml
knot-config = { path = "../knot-config" }
knot-obs = { path = "../knot-obs" }
knot-storage = { path = "../knot-storage" }
```

- [ ] **Step 2: Update `crates/knot-server/src/lib.rs`**

The current `AppState` only holds `Arc<Rooms>`. Extend it with the DB pool:

```rust
//! knot spike server library — exports `router()` for tests.

use std::sync::Arc;

use axum::{
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use knot_crdt::YrsEngine;
use knot_storage::Pool;

pub mod protocol;
pub mod room;
pub mod routes;

use room::Rooms;

#[derive(Clone)]
pub struct AppState {
    pub rooms: Arc<Rooms>,
    pub pool: Option<Pool>,
}

impl AppState {
    pub fn in_memory() -> Self {
        Self {
            rooms: Arc::new(Rooms::new(YrsEngine)),
            pool: None,
        }
    }

    pub fn with_pool(pool: Pool) -> Self {
        Self {
            rooms: Arc::new(Rooms::new(YrsEngine)),
            pool: Some(pool),
        }
    }
}

/// In-memory router (used by tests and the spike main). Plan 5 makes
/// the pool a hard requirement.
pub fn router() -> Router {
    router_with_state(AppState::in_memory())
}

pub fn router_with_state(state: AppState) -> Router {
    Router::new()
        .route("/collab/:doc_id", get(collab_upgrade))
        .merge(routes::health::router())
        .with_state(state)
}

async fn collab_upgrade(
    Path(doc_id): Path<String>,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        state.rooms.serve(doc_id, socket).await;
    })
}
```

The `routes::health::router()` lands in T7.

- [ ] **Step 3: Stub the routes module so the build compiles**

Create `crates/knot-server/src/routes/mod.rs`:

```rust
pub mod health;
```

Create `crates/knot-server/src/routes/health.rs` (just the stub — T7 fills it in):

```rust
//! Health & readiness endpoints. Filled in by Plan 2 Task 7.

use axum::{routing::get, Router};

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new().route("/api/healthz", get(|| async { "ok" }))
}
```

- [ ] **Step 4: Rewrite `crates/knot-server/src/main.rs`**

```rust
//! knot spike server binary.
//!
//! Plan 2 wires layered config + observability + Postgres pool.
//! Plan 5 makes the pool mandatory once CRDT persistence lands.

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::process;

use knot_config::Config;

#[tokio::main]
async fn main() {
    // 1. Load config.
    let cfg = match Config::load(std::env::var("KNOT_CONFIG").ok()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config: {e}");
            process::exit(2);
        }
    };

    // 2. Init observability.
    if cfg.tracing_enabled && !cfg.otlp_endpoint.is_empty() {
        if let Err(e) = knot_obs::tracing::init_with_otlp(
            &cfg.log_level,
            &cfg.log_format,
            &cfg.otlp_endpoint,
            "knot-server",
        ) {
            eprintln!("tracing init: {e}");
            process::exit(2);
        }
    } else if let Err(e) = knot_obs::logging::init(&cfg.log_level, &cfg.log_format) {
        eprintln!("logging init: {e}");
        process::exit(2);
    }
    if let Err(e) = knot_obs::metrics::init(&cfg.metrics_addr) {
        tracing::warn!(error=?e, "metrics init failed; continuing without /metrics");
    }

    // 3. Connect to Postgres if configured.
    let pool = if !cfg.database_url.is_empty() {
        match knot_storage::connect(&cfg.database_url, 16).await {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::error!(error=?e, "database connect failed");
                process::exit(3);
            }
        }
    } else {
        tracing::warn!("KNOT_DATABASE_URL not set; running in-memory only");
        None
    };

    // 4. Build router.
    let state = match pool {
        Some(p) => knot_server::AppState::with_pool(p),
        None => knot_server::AppState::in_memory(),
    };
    let app = knot_server::router_with_state(state);

    // 5. Bind + serve.
    let listener = match tokio::net::TcpListener::bind(normalize_addr(&cfg.addr)).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!(error=?e, addr=%cfg.addr, "bind failed");
            process::exit(4);
        }
    };
    tracing::info!(addr=%listener.local_addr().unwrap(), "listening");
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!(error=?e, "serve failed");
        knot_obs::tracing::shutdown();
        process::exit(5);
    }
    knot_obs::tracing::shutdown();
}

fn normalize_addr(addr: &str) -> String {
    if let Some(port) = addr.strip_prefix(':') {
        if port.parse::<u16>().is_ok() {
            return format!("0.0.0.0:{port}");
        }
    }
    addr.to_string()
}
```

- [ ] **Step 5: Build + run existing tests**

```
cargo build --workspace
cargo nextest run --workspace
```

Expected: all previous tests still PASS (smoke, convergence, knot-markdown round_trip, schemagen goldens, knot-storage migrations_apply if Docker is available, knot-config tests, knot-obs unit test).

- [ ] **Step 6: Manual smoke**

```
make compose.up
KNOT_DATABASE_URL=postgres://knot:knot@localhost:5432/knot cargo run --bin knot-server &
SERVER=$!
sleep 3
curl -s http://localhost:3000/api/healthz
echo
curl -s http://localhost:9090/metrics | head -10
kill $SERVER
make compose.down
```

Expected:
- `healthz` returns `ok`.
- `:9090/metrics` returns Prometheus-format text starting with `# HELP` or similar.

If `:9090/metrics` is silent: the metrics exporter may not have started in time. Adjust the order of init calls so metrics init blocks until the listener is up (current `init` returns synchronously after installing).

- [ ] **Step 7: Lint + commit**

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/knot-server Cargo.lock
git commit -m "feat(knot-server): wire config + obs + storage into main"
```

---

## Task 7: `/api/healthz`, `/api/readyz`, `/api/version`

**Files:**
- Modify: `crates/knot-server/src/routes/health.rs`
- Modify: `crates/knot-server/build.rs` (new — for VERSION/COMMIT env vars)
- Create: `crates/knot-server/tests/health_integration.rs`

- [ ] **Step 1: Add build script**

`crates/knot-server/build.rs`:

```rust
//! Stamp the binary with build-time metadata (version + commit).

use std::process::Command;

fn main() {
    let version = std::env::var("KNOT_VERSION").unwrap_or_else(|_| "dev".to_string());
    let commit = std::env::var("KNOT_COMMIT").unwrap_or_else(|_| {
        Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".into())
    });
    println!("cargo:rustc-env=KNOT_BUILD_VERSION={version}");
    println!("cargo:rustc-env=KNOT_BUILD_COMMIT={commit}");
    println!("cargo:rerun-if-env-changed=KNOT_VERSION");
    println!("cargo:rerun-if-env-changed=KNOT_COMMIT");
}
```

Reference the build script in `crates/knot-server/Cargo.toml` `[package]` section:

```toml
build = "build.rs"
```

- [ ] **Step 2: Replace `crates/knot-server/src/routes/health.rs`**

```rust
//! Health & meta endpoints.
//!
//! - `/api/healthz` — liveness; always 200 if the process is running.
//! - `/api/readyz` — readiness; 200 only if DB is reachable.
//! - `/api/version` — build metadata.

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use serde::Serialize;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/healthz", get(healthz))
        .route("/api/readyz", get(readyz))
        .route("/api/version", get(version))
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let Some(pool) = state.pool.as_ref() else {
        // No DB configured: report ready (in-memory mode is fine for the spike).
        return (StatusCode::OK, "ok (in-memory)").into_response();
    };
    match sqlx::query("SELECT 1").execute(pool).await {
        Ok(_) => (StatusCode::OK, "ok").into_response(),
        Err(e) => {
            tracing::warn!(error=?e, "readyz: db check failed");
            (StatusCode::SERVICE_UNAVAILABLE, "db unavailable").into_response()
        }
    }
}

#[derive(Serialize)]
struct VersionInfo {
    version: &'static str,
    commit: &'static str,
}

async fn version() -> impl IntoResponse {
    Json(VersionInfo {
        version: env!("KNOT_BUILD_VERSION"),
        commit: env!("KNOT_BUILD_COMMIT"),
    })
}
```

This requires `AppState` to be the generic parameter; update `routes::health::router()` callers in `lib.rs` accordingly (the stub from T6 used a generic; tighten to `Router<AppState>` so the State extractor works).

If you get type errors around `Router<S>` vs `Router<AppState>`, switch the stub in `routes/mod.rs` to import `AppState` from `crate::AppState` and tighten the type.

- [ ] **Step 3: Integration test**

`crates/knot-server/tests/health_integration.rs`:

```rust
//! Integration tests for health endpoints.

use std::time::Duration;
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread")]
async fn healthz_returns_ok_without_pool() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = knot_server::router();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(20)).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/api/healthz"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");
}

#[tokio::test(flavor = "multi_thread")]
async fn readyz_returns_ok_without_pool() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = knot_server::router();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(20)).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/api/readyz"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn version_returns_json() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = knot_server::router();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(20)).await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/api/version"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("version").is_some());
    assert!(body.get("commit").is_some());
}
```

Add `reqwest` and `serde_json` to `crates/knot-server/Cargo.toml [dev-dependencies]`:

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde_json = "1"
```

- [ ] **Step 4: Run tests**

```
cargo nextest run -p knot-server
```

Expected: 4 tests pass (the 2 from R-T12 + 3 new = total 5). Wait — `smoke::dial_succeeds` + `convergence::two_clients_converge` + `health_integration::healthz_returns_ok_without_pool` + `readyz_returns_ok_without_pool` + `version_returns_json` = 5 integration tests. Plus the 2 protocol unit tests. Total 7. Confirm the count matches.

- [ ] **Step 5: Lint + commit**

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/knot-server
git commit -m "feat(knot-server): /api/healthz, /api/readyz, /api/version"
```

---

## Task 8: Extend e2e to verify health endpoints

**Files:**
- Modify: `e2e/flows/two-users-converge.spec.ts` (preserve existing test)
- Create: `e2e/flows/health.spec.ts`

- [ ] **Step 1: Add health spec**

`e2e/flows/health.spec.ts`:

```ts
import { test, expect, request } from "@playwright/test";

test("health endpoints respond", async () => {
  const ctx = await request.newContext({ baseURL: "http://localhost:3000" });

  const healthz = await ctx.get("/api/healthz");
  expect(healthz.status()).toBe(200);
  expect(await healthz.text()).toBe("ok");

  const readyz = await ctx.get("/api/readyz");
  expect(readyz.status()).toBe(200);

  const version = await ctx.get("/api/version");
  expect(version.status()).toBe(200);
  const body = await version.json();
  expect(body.version).toBeTruthy();
  expect(body.commit).toBeTruthy();
});
```

- [ ] **Step 2: Run e2e**

```
cd /home/nik/Development/knot && make e2e
```

Expected: 2 tests run (existing `two-users-converge` + new `health endpoints respond`). Both PASS.

- [ ] **Step 3: Commit**

```
git add e2e/flows
git commit -m "test(e2e): verify /api/healthz, /api/readyz, /api/version"
```

---

## Task 9: GitHub Actions CI baseline

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Create the workflow**

`.github/workflows/ci.yml`:

```yaml
name: ci

on:
  push:
    branches: [main, master]
  pull_request:

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"

jobs:
  fmt-and-lint:
    name: fmt + clippy + tsc
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt,clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets --all-features -- -D warnings
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
          cache-dependency-path: web/pnpm-lock.yaml
      - run: pnpm install --frozen-lockfile
        working-directory: web
      - run: pnpm tsc --noEmit
        working-directory: web

  unit-rust:
    name: cargo nextest
    runs-on: ubuntu-latest
    services:
      # Make Docker available for testcontainers via host mounting.
      # Actions runners already have Docker; no separate service needed.
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: taiki-e/install-action@nextest
      - run: cargo nextest run --workspace --all-features

  unit-web:
    name: vitest
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
          cache-dependency-path: web/pnpm-lock.yaml
      - run: pnpm install --frozen-lockfile
        working-directory: web
      - run: pnpm test
        working-directory: web

  e2e:
    name: playwright
    runs-on: ubuntu-latest
    needs: [unit-rust, unit-web]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 22
          cache: pnpm
      - run: pnpm install --frozen-lockfile
        working-directory: web
      - run: pnpm install --frozen-lockfile
        working-directory: e2e
      - run: pnpm playwright install --with-deps chromium
        working-directory: e2e
      - run: cargo build --bin knot-server --release
      - name: run playwright
        working-directory: e2e
        env:
          # Use the cargo-built binary directly instead of `cargo run`
          # to avoid the recompile in the webServer step.
          KNOT_TEST_BIN: ../target/release/knot-server
        run: pnpm test

  helm-lint:
    name: helm chart lint
    runs-on: ubuntu-latest
    if: ${{ false }} # Helm chart lands in Plan 9
    steps:
      - uses: actions/checkout@v4

  deny:
    name: cargo-deny
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: taiki-e/install-action@cargo-deny
      - run: cargo deny check
```

The `KNOT_TEST_BIN` env var is a forward hook — the e2e Playwright config doesn't honour it yet. T9b updates `e2e/playwright.config.ts` to use it.

- [ ] **Step 2: Update e2e Playwright config to honour `KNOT_TEST_BIN`**

`e2e/playwright.config.ts` — find the `webServer` array and change the first entry:

```ts
{
  command: process.env.KNOT_TEST_BIN ?? "cargo run --bin knot-server",
  cwd: "..",
  port: 3000,
  reuseExistingServer: !process.env.CI,
  timeout: 180_000,
  stdout: "pipe",
  stderr: "pipe",
},
```

- [ ] **Step 3: Lint the workflow locally (optional)**

If `actionlint` is in the dev shell, run:

```
actionlint .github/workflows/ci.yml
```

Otherwise skip.

- [ ] **Step 4: Commit**

```
git add .github/workflows e2e/playwright.config.ts
git commit -m "ci: github actions baseline (fmt + lint + test + e2e + deny)"
```

The deny job will fail until T10 lands `deny.toml`. That's expected — push triggers CI; the deny job will be red until T10 commits.

---

## Task 10: `cargo-deny` config

**Files:**
- Create: `deny.toml`

- [ ] **Step 1: Create `deny.toml`**

```toml
# cargo-deny configuration for the knot workspace.

[graph]
all-features = true

[advisories]
db-path = "~/.cargo/advisory-db"
db-urls = ["https://github.com/rustsec/advisory-db"]
yanked = "deny"
ignore = []

[licenses]
allow = [
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "MIT",
    "MIT-0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unlicense",
    "Zlib",
    "CC0-1.0",
    "Unicode-3.0",
    "MPL-2.0",
    "OpenSSL",
]
confidence-threshold = 0.93

[[licenses.exceptions]]
# ring uses a custom not-quite-OpenSSL license that cargo-deny recognises
# as a known exception. Add other case-by-case exceptions here.
name = "ring"
allow = ["OpenSSL"]

[bans]
multiple-versions = "warn"
wildcards = "deny"
deny = []
skip = []
skip-tree = []

[sources]
unknown-registry = "deny"
unknown-git = "warn"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

- [ ] **Step 2: Verify locally**

```
cargo deny check
```

Expected: PASS (no advisories, all licenses on the allow list). If a real-world dependency uses a license not on the list, either:
1. Add the license to `licenses.allow` if it's acceptable (Apache/MIT-family).
2. Add an explicit exception under `[[licenses.exceptions]]` for one-off cases.
3. Replace the dependency.

Common findings on first run:
- `unicode-ident` (Unicode-3.0) — allow.
- `webpki-roots` / `rustls-webpki` — check the exact name.
- ring may need the exception above.

Iterate until clean.

- [ ] **Step 3: Commit**

```
git add deny.toml
git commit -m "ci: cargo-deny config"
```

---

## Task 11: Plan 2 outcome doc + update spec roadmap

**Files:**
- Create: `docs/superpowers/research/2026-06-02-plan2-outcome.md`
- Modify: `docs/superpowers/specs/2026-06-01-knot-foundation-design.md` (only if API drift forced changes — most likely OTLP setup if the opentelemetry version differed)

- [ ] **Step 1: Write the outcome doc**

`docs/superpowers/research/2026-06-02-plan2-outcome.md`:

```markdown
# Plan 2 (Repo bootstrap & DB) outcome — 2026-06-02

## What landed

- Postgres 16 dev compose at `deploy/compose/dev.yml` with healthcheck.
- `migrations/20260602000001_v0_1_schema.sql` — all 11 v0.1 tables in one initial migration.
- `crates/knot-config` — figment-based layered loader (defaults < file < KNOT_* env). Validation on production session key.
- `crates/knot-obs` — `tracing` + `metrics` + optional OTLP. JSON or text logs.
- `crates/knot-storage` — `PgPool` factory + migrations runner + `DocStore` trait stub. testcontainers integration test asserts the schema lands cleanly.
- `crates/knot-server` rewired: main is now load-config → init-obs → connect-db → serve.
- `/api/healthz`, `/api/readyz`, `/api/version` endpoints.
- GitHub Actions CI: fmt + clippy + tsc + cargo nextest + vitest + Playwright + cargo-deny.

## Crate count

| Crate | Purpose |
|---|---|
| `tools/schemagen` | JSON → Rust+TS schema codegen (Plan 1) |
| `crates/knot-crdt` | Engine trait + yrs adapter (Plan 1) |
| `crates/knot-markdown` | MD round-trip (Plan 1) |
| **`crates/knot-config`** | layered config (Plan 2, new) |
| **`crates/knot-obs`** | tracing/metrics/OTLP (Plan 2, new) |
| **`crates/knot-storage`** | sqlx pool + DocStore (Plan 2, new) |
| `crates/knot-server` | axum binary (Plan 2 refactor) |

## API drift encountered

(Fill in per actual implementation.)

- `opentelemetry-otlp` 0.x: ...
- `sqlx` 0.8 migrate macro: ...
- `testcontainers-modules` 0.x: ...

## Foundation spec edits

(List any spec sections updated based on reality.)

## Verdict

GO. Proceed to Plan 3 (Auth).
```

- [ ] **Step 2: Apply spec edits inline (if any)**

If the OTLP setup or sqlx API forced a change to spec §11.7 or §11.3, edit those sections to match reality. Note each edit in the outcome doc.

- [ ] **Step 3: Tag**

```
git add docs/superpowers/research/2026-06-02-plan2-outcome.md docs/superpowers/specs/2026-06-01-knot-foundation-design.md
git commit -m "docs: Plan 2 outcome (and any spec edits)"
git tag plan-2-complete
```

- [ ] **Step 4: Final gate run**

```
make lint
make test
make e2e
cargo deny check
```

Expected: ALL green. If any fails, fix before declaring Plan 2 done.

---

## What this plan deliberately does not do

- **No auth.** Anyone with TCP reach to `:3000` can still connect. Plan 3 adds local + OIDC.
- **No document tree / ACL.** Plan 4.
- **No CRDT room actor / persistence.** The Plan 1 in-memory `Rooms` still serves convergence. Plan 5 replaces it with the actor model + `doc_updates`/`doc_snapshots` persistence + Postgres LISTEN/NOTIFY Bus.
- **No frontend changes** beyond Playwright tests for the new endpoints.
- **No Helm / Docker image build.** Plan 9.

These constraints are intentional. Plan 2's job is the infrastructure that Plans 3-9 build on, nothing more.

---

## Quick reference: the production `knot-server` startup sequence after Plan 2

```
                       1. Config::load(KNOT_CONFIG opt.)
                                  │
                                  ▼
                  ┌────────────────────────────┐
                  │ 2. knot_obs::tracing or    │
                  │    knot_obs::logging init  │
                  │    + knot_obs::metrics     │
                  └─────────────┬──────────────┘
                                ▼
                  ┌────────────────────────────┐
                  │ 3. knot_storage::connect   │
                  │    (pool + migrate)        │
                  └─────────────┬──────────────┘
                                ▼
                  ┌────────────────────────────┐
                  │ 4. router_with_state(...)  │
                  │    /api/{healthz,readyz,   │
                  │      version}              │
                  │    /collab/:doc_id (WS)    │
                  └─────────────┬──────────────┘
                                ▼
                  ┌────────────────────────────┐
                  │ 5. axum::serve(listener,   │
                  │    app).await              │
                  └────────────────────────────┘
```

Plans 3-5 attach more routes and middleware to step 4. Plans 6-8 are frontend changes. Plan 9 is deploy.
