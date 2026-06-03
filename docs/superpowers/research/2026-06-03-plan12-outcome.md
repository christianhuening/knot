# Plan 12 Outcome — Production Hardening

**Status:** GO. 5/6 tasks landed cleanly; T5 (WS reconnect e2e) was skipped with a documented Playwright limitation. All other gates green.

**Verdict:** knot is materially safer to run in front of strangers now — `/auth/password` shares the existing token-bucket throttle, the Helm chart can deny everything-by-default at the pod level, and a Prometheus user gets burn-rate alerts that match the SLO doc. The release workflow turns a `v*` tag into pushed multi-arch images. Recommended next: **Plan 13 (file uploads)** or **Plan 14 (full-text search)**.

## What landed

| Commit | Task | Subject |
|---|---|---|
| 4c8afb9 | T1 | `knot-server`: throttle `/auth/password` by user+ip |
| d1970e2 | T2 | `deploy`: optional default-deny NetworkPolicy template |
| a22f05c | T3 | `deploy`: PrometheusRule template with SLO burn-rate alerts |
| cb25adc | T4 | `ci`: release workflow — multi-arch image push + chart artifact on tag |
| 2b9d534 | T5 | `test(e2e)`: WS reconnect spec (skipped — Playwright limitation documented) |

T6 is this outcome doc.

## Gates

- `cargo test --workspace` — green (+1 new throttle case in `auth_password_integration.rs`)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean
- `pnpm tsc/lint/test` — clean
- `pnpm playwright test` — **19 passed, 1 skipped** (the documented `ws-reconnect.spec.ts`)
- `helm lint` with all toggles enabled (`networkPolicy.enabled=true`, `serviceMonitor.enabled=true`, `alerting.enabled=true`) — clean
- `helm template ... | kubectl apply --dry-run=client -f -` — accepts native resources; `PrometheusRule` errors with "unknown kind" on clusters without kube-prometheus-stack CRDs (expected and matches the existing `ServiceMonitor` behaviour)

## Architecture summary

**Throttle on `/auth/password`:** mirrors the existing `login` pattern. Keys are `pw:ip:<ip>` and `pw:user:<uuid>`. `record_failure` on wrong-password (both keys) and on OIDC-only users (IP only — keeps user existence non-discoverable). `reset` on successful change. 429 with `auth.throttled` code when either bucket is empty. Default capacity (5) and 1-token-per-minute drain come from `knot_auth::throttle`.

**NetworkPolicy (default-deny):** gated by `networkPolicy.enabled`. Pod selector matches the chart's standard selector labels. Two ingress rules:
1. http port 3000 from `networkPolicy.ingressFrom` (default `{}/{}` = anywhere, since most clusters use an ingress controller in front)
2. metrics port from `networkPolicy.metricsIngressFrom` (default: pods labeled `app.kubernetes.io/name: prometheus`)

No egress policy — Postgres, Dex, OTLP, OIDC discovery all need it and getting that right is org-specific. Documented as deferred.

**PrometheusRule:** gated by `metrics.enabled AND serviceMonitor.enabled AND alerting.enabled`. Three alerts straight from `docs/SLO.md`:
- `KnotErrorBudgetFastBurn` — 1h 5xx rate × (1 − target) × 14.4; for 5m; page severity
- `KnotErrorBudgetSlowBurn` — 6h 5xx rate × (1 − target) × 6; for 30m; ticket severity
- `KnotLatencyP95High` — P95 above target for 30m; ticket severity

All exclude `route=~"/api/health.*"` so probe traffic doesn't dilute the signal.

**Release workflow:** triggered on `push: tags: v*`. Two jobs:
1. `image`: buildx multi-arch (amd64+arm64) push to `ghcr.io/<repo>:<tag>` + `:<minor>` + `:latest`, with GHA cache.
2. `chart`: needs `image`; runs `helm package` and uploads the `.tgz` as a workflow artifact (retention 90 days). OCI chart push is a small follow-up.

**WS reconnect e2e — skipped:** `context.setOffline(true)` blocks new connections in Chromium but does **not** synchronously close the existing WebSocket. The browser dispatches the offline transition to active WS instances via TCP keepalive timeout (30s+), so the test couldn't observe the `offline` status within any reasonable timeout. Alternatives ruled out: `page.route()` doesn't intercept WS frames, exposing `KnotProvider` on `window` is invasive, server SIGKILL kills the whole suite. The `KnotProvider.scheduleReconnect` path is exercised in production by NAT timeouts + server restarts; a follow-up should use a dedicated WS midfield proxy (e.g. `toxiproxy`) to simulate the flap deterministically.

## What was non-obvious

**The throttle was already there.** A token-bucket `Throttle` lives in `crates/knot-auth/src/throttle.rs` and `/auth/login` already uses it. The "rate limit auth endpoints" carryforward from Plan 11 turned out to be a 30-line copy-paste pattern, not a new feature. The integration test landed in the existing `auth_password_integration.rs` and reused the same `state_with_seeded_user` / `login` helpers, so the diff stayed small.

**Helm `sub` is float-tolerant.** `{{- $budget := sub 1.0 .Values.alerting.errorBudgetTarget -}}` produces `0.0049999...` when target is `0.995`. Prometheus parses it fine. No need for fmt rounding.

**Playwright's offline mode is a leaky abstraction.** The setOffline call is essentially "don't allow new outbound connections" — it doesn't proactively tear down existing sockets. This bit us once already on the OIDC e2e (the WS proxy through Vite was independent of the page's HTTP context). Documented the alternative paths inline in the skipped spec so the next person doesn't reinvent them.

**`PrometheusRule` + `kubectl --dry-run=client` mismatch is expected.** Both `ServiceMonitor` and now `PrometheusRule` 422 the client-side dry-run on a vanilla cluster because the kube-prometheus-stack CRDs aren't installed. The chart's render is correct; the validation is the wrong tool. Documented.

## What's still deferred

- **Account lockout** (hard lock for 24h after N failures). The token bucket already gives natural backoff; a hard lockout adds operational pain (admin unlock flow). Defer until needed.
- **Cosign image signing.** Once the release cadence is established.
- **Helm chart OCI push** (`helm push dist/knot-*.tgz oci://ghcr.io/<owner>/charts`). One additional step in the `chart` job; left as workflow-artifact-only for v0.1.
- **Egress NetworkPolicy.** Postgres + Dex + OTLP + OIDC all need it; getting it right is per-cluster.
- **Synthetic probes / blackbox monitoring.** Separate plan.
- **Per-replica multi-pod throttle.** The bucket is per-process; a distributed attacker could round-robin replicas. Move to Postgres-backed counter when needed.
- **WS reconnect e2e with toxiproxy.** Deterministic flap simulation. Better as part of a Plan 12.5 / chaos-testing follow-up.

## Carryforward for the next plan

In recommended priority order:

1. **Plan 13 — File uploads / image embeds.** Notion-style images and attachments in docs. Needs a blob storage decision (Postgres large objects vs. S3-compatible). Touches the editor schema (new node type), the server (POST /api/blobs?), and the chart (PVC vs. external bucket reference).
2. **Plan 14 — Full-text search.** Postgres FTS over `doc_markdown_cache` for v0.1. A `tantivy`-based index is a follow-up once relevance becomes a concern.
3. **Plan 12.5 — Chaos coverage.** Toxiproxy-based WS flap test, Postgres restart drill, NetworkPolicy verification with kind-installed Cilium.

## Files of interest

| Path | Role |
|---|---|
| `crates/knot-server/src/routes/auth/local.rs` | throttle keys + checks + reset on `change_password` |
| `crates/knot-server/tests/auth_password_integration.rs` | 6/6 cases including the 429 throttle case |
| `deploy/helm/knot/templates/networkpolicy.yaml` | dual-ingress (http + metrics) default-deny |
| `deploy/helm/knot/templates/prometheusrule.yaml` | SLO burn-rate alerts |
| `deploy/helm/knot/values.yaml` | `networkPolicy.*`, `alerting.*` blocks |
| `deploy/helm/knot/values.schema.json` | matching JSON schema |
| `.github/workflows/release.yaml` | tag-triggered multi-arch image push + chart artifact |
| `e2e/flows/ws-reconnect.spec.ts` | skipped with the limitation documented inline |
