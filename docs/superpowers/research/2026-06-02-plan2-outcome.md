# Plan 2 (Repo bootstrap & DB) outcome — 2026-06-02

## What landed

- **Postgres 16 dev compose** at `deploy/compose/dev.yml` with `pg_isready` healthcheck and named volume.
- **`migrations/20260602000001_v0_1_schema.sql`** — all 11 v0.1 tables in one initial migration (workspaces, users, workspace_members, sessions, documents, document_grants, doc_updates, doc_snapshots, doc_markdown_cache, audit_events, acl_invalidations).
- **`crates/knot-config`** — figment-based layered loader (defaults < optional yaml file < `KNOT_*` env vars). 4 integration tests via `figment::Jail`. Validates production session-key requirement and log_level/log_format enums.
- **`crates/knot-obs`** — `tracing-subscriber` + `metrics-exporter-prometheus` + optional `opentelemetry-otlp` OTLP exporter. JSON or text logs. Three independent modules so a binary can opt into any subset.
- **`crates/knot-storage`** — `PgPool` factory + migrations runner + `DocStore` trait stub. `testcontainers-modules::postgres` integration test asserts the schema lands cleanly against a fresh DB.
- **`crates/knot-server` rewired** — `main.rs` is now load-config → init-obs → connect-DB → serve. `AppState` carries `Option<Pool>`.
- **`/api/healthz`, `/api/readyz`, `/api/version`** — readyz pings DB when configured; version emits build-time stamped metadata via `build.rs`.
- **GitHub Actions CI** at `.github/workflows/ci.yml` — fmt + clippy + tsc + cargo nextest + vitest + Playwright + cargo-deny across 5 parallel jobs.
- **`deny.toml`** — license allowlist, advisory ignores (tokio-tar + rustls-pemfile, both dev-deps-only via testcontainers), `allow-wildcard-paths` for workspace internals.

## Workspace at end of Plan 2

```
knot/
├── tools/schemagen           Plan 1 — JSON → Rust+TS codegen
├── crates/knot-crdt          Plan 1 — Engine trait + yrs adapter
├── crates/knot-markdown      Plan 1 — MD round-trip
├── crates/knot-config        Plan 2 — figment layered config           ★ new
├── crates/knot-obs           Plan 2 — tracing/metrics/OTLP             ★ new
├── crates/knot-storage       Plan 2 — sqlx pool + DocStore             ★ new
└── crates/knot-server        Plan 2 — rewired main; routes::health     ★ refactor
```

## API drift encountered

- **`opentelemetry-otlp` 0.27.1** vs docs.rs:
  - `SdkTracerProvider` doesn't exist yet — actual type is `TracerProvider`.
  - `with_batch_exporter(exporter, runtime::Tokio)` still requires the runtime arg (docs implied no runtime).
  - `Resource::builder_empty()` doesn't exist — use `Resource::new([kvs])`.
  - `global::shutdown_tracer_provider()` was removed — call `provider.shutdown()` on the returned provider instead. `knot-obs::tracing::init_with_otlp` now returns `Result<TracerProvider, _>` so the caller holds the provider for its lifetime.
- **`testcontainers-modules` 0.11.6** — no drift. The `testcontainers_modules::testcontainers::runners::AsyncRunner` re-export + `Postgres::default()` + `get_host_port_ipv4(5432).await` all matched the plan exactly.
- **`sqlx` 0.8** — no drift. `sqlx::migrate!("../../migrations")` resolves correctly from the crate.
- **`figment` 0.10** — no drift on env mapping. `Env::prefixed("KNOT_")` maps `KNOT_DATABASE_URL` → `database_url` field directly without `.split("_")`.

## Cargo-deny findings + resolutions

- **`tokio-tar` (RUSTSEC-2025-0111)** — tar parser vuln in archived crate. Reachable only via `testcontainers > bollard`. Ignored in `[advisories]` with a dev-deps-only rationale.
- **`rustls-pemfile` (RUSTSEC-2025-0134)** — unmaintained; superseded by `rustls-pki-types`. Transitive via the same `testcontainers > bollard` chain. Ignored.
- **`webpki-roots` license CDLA-Permissive-2.0** — Linux Foundation Community Data License, permissive. Added to the licenses.allow list.
- **Workspace crates flagged as "unlicensed" + "wildcard"** — fixed by adding `license.workspace = true` + `publish = false` to all 7 member Cargo.toml [package] sections, plus `allow-wildcard-paths = true` in deny.toml.

## Foundation spec edits

None. The spec's design held up against reality. The OTLP method-name drift documented above is a tactical detail not worth editing into §11.7 (which describes capability, not API).

## Test counts at Plan 2 close

```
cargo nextest run --workspace      → 28/28 PASS (up from 18 at Plan 1 close)
  + 4 knot-config (load tests)
  + 2 knot-obs (metrics::normalize_addr)
  + 1 knot-storage (migrations_apply)
  + 3 knot-server (health_integration)

cd e2e && pnpm test                → 2/2 PASS
  + two-users-converge.spec.ts
  + health.spec.ts (new)

cargo deny check                   → advisories + bans + licenses + sources all ok
```

## Performance / size

- `cargo build --release --bin knot-server` warm cache: ~5s.
- Cold build (after `cargo clean`): ~2-3 min, dominated by `tokio` + `opentelemetry` deps.
- `target/release/knot-server` ≈ 4-5 MB (still small; mimalloc adds ~200KB over baseline).
- `make compose.up` + healthcheck: ~3-4s on warm Docker cache.
- testcontainers `migrations_apply_cleanly` test: ~1.5s warm cache, ~10-30s cold (postgres pull).

## Plan 2 commit trail (master)

```
72280c7 ci: github actions baseline (fmt + lint + test + e2e + deny)
29d1664 ci: cargo-deny config + license/advisory ignores; license.workspace + publish=false
e593a12 test(e2e): verify /api/healthz, /api/readyz, /api/version
b3da787 feat(knot-server): /api/healthz, /api/readyz, /api/version
3799a4b feat(knot-server): wire config + obs + storage into main
b879519 feat(knot-storage): Postgres pool + migration runner + testcontainers smoke
6c513fa feat(db): initial v0.1 schema migration (all 11 tables)
8729871 feat(knot-obs): tracing + metrics + OTLP setup helpers
e299690 fix(knot-config): allow result_large_err in figment Jail tests
20170e1 feat(knot-config): figment-based layered config loader
0a3456f feat(dev): postgres compose stack + Makefile targets
```

(11 commits for 11 tasks. Tag: `plan-2-complete`.)

## Verdict

**GO.** All architectural primitives the rest of Foundation needs are in place:

- Config layer with strict-mode production guard.
- Observability that's usable on day 1 (Prometheus metrics + structured logs) and ready for OTLP when an aggregator is in scope.
- A Postgres pool + migrations runner with a tested smoke path.
- Health/readiness endpoints for orchestrators (k8s probes, load balancers).
- CI gate that runs every check developers actually care about.
- Supply-chain hygiene via cargo-deny with documented exceptions.

Proceed to **Plan 3 (Auth)**: local email/password + sessions + OIDC + Dex dev integration.

## What's still NOT done after Plan 2

Carrying forward to later plans:
- No auth (Plan 3).
- No document tree / ACL (Plan 4).
- No CRDT room actor / persistence — the spike's in-memory `Rooms` still serves convergence; Plan 5 replaces it.
- No frontend changes beyond the e2e health spec.
- No Helm chart / Docker image build / multi-arch release (Plan 9).

These are intentional. Plan 2's job was the infrastructure that Plans 3-9 build on, nothing more.
