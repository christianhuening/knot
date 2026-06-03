# Plan 9 Outcome — Deployment

**Status:** GO. All 12 tasks landed; all gates green.

**Verdict:** knot is now installable on a stock Kubernetes cluster with `helm install` (assuming an external Postgres and optional external OIDC). The image is ~20 MB, multi-arch (amd64 + arm64) via static musl compilation. Recommended next: **Plan 7 (UI polish)** or **Plan 10 (observability)**.

## What landed

Plan 9 commits (HEAD `7d9db3f`):

| Commit | Task | Subject |
|---|---|---|
| 0174f0d | T1  | `knot-server migrate` subcommand for k8s Job hook |
| abddb9d | T2  | multi-arch musl Dockerfile + SPA fallback + image Makefile targets |
| d72015c | T2 fix | bump rust base to 1.90-alpine (cargo-zigbuild 0.22.3 needs rustc ≥1.88) |
| 0d8ef80 | T4  | Helm chart skeleton (Chart.yaml + values.yaml + helpers + .helmignore) |
| 34f8070 | T5  | Deployment + Service + ConfigMap templates |
| 6461b2b | T6  | Secret template (dev path; existingSecretName supported) |
| 97dcdac | T7  | Ingress + ServiceAccount templates |
| 53c9d4e | T8  | pre-install + pre-upgrade migration Job |
| 50262af | T9  | Helm test pod (curl /api/healthz) |
| afe5242 | T10 | values.schema.json — validates required inputs |
| 7d9db3f | T11 | GitHub Actions: ct lint + ct install in kind |

T3 was the local smoke test (no commit — verification only). T12 is this outcome doc + the chart README.

## Gates

- `cargo test --workspace` — green
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean
- `pnpm tsc` + `pnpm lint` + `pnpm test` + `pnpm playwright test` — 15/15
- `make image.build.host` — **19.5 MB** scratch image, builds in ~3 minutes from cold cache
- `make image.smoke`:
  - `knot:dev migrate` → exit 0, `migrate: ok`
  - `knot:dev` running → `/api/healthz` returns 200, `/` returns 200 `text/html` (SPA served from `/web/dist`)
- `helm lint deploy/helm/knot --set database.url=x --set session.key=y` — clean
- `helm template ... | kubectl apply --dry-run=client -f -` — accepts all 7 resources (ServiceAccount, Secret, ConfigMap, Service, Deployment, test Pod, migrate Job)

## What was non-obvious

**knot-server didn't serve the SPA.** The Plan 1 spike had Vite serve the SPA in dev; Plan 6 inherited that. T2 added a `.fallback_service(ServeDir::new(KNOT_WEB_DIST))` after the API/auth/collab routes, with `not_found_service(ServeFile::new(index.html))` so React Router client-side routes (`/doc/:id`) fall through to `index.html`. The default `KNOT_WEB_DIST=/web/dist` matches the path the Dockerfile copies the built SPA into.

**cargo-zigbuild needs a new rustc.** The first build attempt with `rust:1.83-alpine` failed because cargo-zigbuild 0.22.3 requires rustc 1.88+. Pinned to `rust:1.90-alpine`. We did not pin cargo-zigbuild because the latest is what we want when this image is rebuilt months from now.

**Migrations are run by `knot_storage::connect()` already.** No new sqlx wiring needed for the migrate subcommand — it just calls the existing pool constructor with a tiny pool (1 conn) and exits when it returns. The same code path runs on `Serve` startup, which means the migration Job is mainly there for blocking the Deployment rollout on migration failure rather than introducing a different code path.

**checksum/config in Deployment forced template ordering.** The Deployment annotation `checksum/config: {{ include (print $.Template.BasePath "/configmap.yaml") . | sha256sum }}` evaluates at lint time, so ConfigMap couldn't be deferred to T6 — landed alongside Deployment in T5. Functionally identical.

## What's still deferred

- **Bundled Postgres + Dex subcharts** — values.yaml documents the external endpoints. `helm install` users either point at an existing Postgres or run the dev-compose stack and tunnel.
- **Image push to a registry on tag** — the build/push wiring is local-only. A small follow-up workflow adds `docker buildx build --push` to a release-on-tag job.
- **Image signing (cosign / sigstore)** — defer to a hardening plan.
- **HPA, PodDisruptionBudget, NetworkPolicy** — for v0.1 a single replica is fine. Multi-replica works (CRDT rooms via Postgres LISTEN/NOTIFY) but needs scale testing.
- **ServiceMonitor for Prometheus** — defer to Plan 10 (observability).
- **`ct install` in CI uses `probes.enabled=false`, `migrations.enabled=false`** — no Postgres in the kind cluster, so the chart gets a structural validation but not a true install-from-zero. A follow-up adds a Postgres sidecar to the kind cluster.

## Carryforward for the next plan

Recommendations:

1. **Plan 10 — Observability.** `knot-obs` is wired (logging + OTLP traces stub + Prometheus metrics endpoint at `:9090`). Plan 10 should: enable OTLP traces by default in the chart, add a ServiceMonitor, ship a sample Grafana dashboard, and document SLOs.
2. **Plan 7 — UI polish.** Drag-drop tree move (POST /api/docs/:id/move already exists), command palette (Zustand slot is wired), per-doc effective-role-aware editor toolbar, mobile pass.
3. **Release automation.** Add a `release.yaml` workflow that on tag push: runs `docker buildx build --push --platform linux/amd64,linux/arm64 -t ghcr.io/voss/knot:${tag}` and `helm package deploy/helm/knot` + uploads the chart to a `gh-pages` Helm repo or OCI registry.

## Files of interest

| Path | Role |
|---|---|
| `crates/knot-server/src/main.rs` | new `Cmd::Migrate` subcommand |
| `crates/knot-server/src/lib.rs` | `.fallback_service(ServeDir)` for the SPA |
| `Dockerfile` | 3-stage musl multi-arch build |
| `.dockerignore` | excludes test/dev artifacts |
| `Makefile` | `image.build`, `image.build.host`, `image.smoke` targets |
| `deploy/helm/knot/` | the chart |
| `deploy/helm/knot/values.schema.json` | required-field validation |
| `deploy/helm/knot/README.md` | install runbook |
| `.github/workflows/helm-ci.yaml` | ct lint + ct install in kind on PR |
