# Production Hardening Implementation Plan (Plan 12)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Close the production-readiness gaps called out in the Plan 9/10/11 outcome docs without expanding the feature surface. Five surfaces:
1. Rate-limit `/auth/password` (`/auth/login` already throttled by `knot-auth::Throttle`; `/auth/password` is not).
2. Default-deny NetworkPolicy in the Helm chart, with allow-rules for http + metrics.
3. PrometheusRule template that codifies the SLO burn-rate signals in `docs/SLO.md` as alert expressions.
4. Image push workflow on git tag (multi-arch buildx + push to GHCR).
5. e2e for WS reconnect-on-network-flap (the `KnotProvider.scheduleReconnect` path landed in Plan 6 but is uncovered).

**Architecture:**
- **Throttle on `/auth/password`** — same pattern as `/auth/login` already in `crates/knot-server/src/routes/auth/local.rs`. Key the throttle by the **authenticated user's id** (we have `AuthContext`) plus the client IP. Record a failure on bad current password, reset on success. 429 on `Allow::No`.
- **NetworkPolicy** — one template, gated by `networkPolicy.enabled` (default `false` for backward compat). When enabled it defines: default-deny ingress on pod-selector, then allow ingress on `http` (from an `allowedFrom` namespaceSelector + podSelector list) + allow ingress on `metrics` (from a separate `metricsAllowedFrom` list, typically the monitoring namespace).
- **PrometheusRule** — single template gated by `serviceMonitor.enabled` AND `alerting.enabled`. Three alert groups: fast burn (×14.4), slow burn (×6), latency burn (P95 above target for 30 m). Values from `docs/SLO.md`.
- **Image push CI** — new `.github/workflows/release.yaml` triggered on tag push (`v*`). Logs into GHCR, runs `docker buildx build --push --platform linux/amd64,linux/arm64 -t ghcr.io/voss/knot:${tag} -t ghcr.io/voss/knot:latest`. Optionally `helm package` + upload.
- **WS reconnect e2e** — Playwright spec that uses `page.context().setOffline(true)` mid-edit, asserts `status-dot=offline`, then `setOffline(false)` and asserts back to `connected` + content still present.

**Tech Stack:** No new Rust or TS deps. CI deps: `docker/login-action`, `docker/setup-buildx-action`, `docker/setup-qemu-action`.

**Predecessor:** Plan 11 (developer experience, HEAD `575f60a`).

**Out of scope:**
- **WAF / Cloudflare-style edge rate limiting** — application-level throttle is enough for v0.1.
- **Account lockout policy** (lock for 24h after N failures) — the existing token bucket gives backoff naturally; a hard lockout adds operational pain (admin unlock flow). Defer.
- **Cosign image signing** — separate hardening plan once we have a release cadence.
- **Helm chart signing** — same.
- **Distroless variant for image** — scratch is already minimal.
- **Multi-architecture e2e** — single-arch CI is enough; the cross-build proves the build, not the runtime.

---

## File map

```
crates/knot-server/
├── src/routes/auth/local.rs                    (modify) throttle change_password handler
└── tests/auth_password_integration.rs          (modify) +case: throttle returns 429 after N failures

deploy/helm/knot/
├── values.yaml                                 (modify) +networkPolicy + alerting blocks
├── values.schema.json                          (modify) new keys
└── templates/
    ├── networkpolicy.yaml                       (new)
    └── prometheusrule.yaml                      (new)

.github/workflows/
└── release.yaml                                (new) tag-triggered multi-arch image push

e2e/flows/
└── ws-reconnect.spec.ts                        (new) offline → online round-trip
```

---

## Conventions

- Throttle keying for `/auth/password`: combine `user_id` + a stable IP key (existing `client_ip` helper in `local.rs`). Two `record_failure` calls per bad password; `reset` both on success.
- NetworkPolicy uses `app.kubernetes.io/name`-based selectors so it auto-targets the right Pod.
- PrometheusRule uses `route!~"/api/health.*"` to exclude probe traffic from the error-budget burn signals.
- Release workflow uses `${{ github.ref_name }}` for the tag (`v0.1.0` etc.). Pushes `:vX.Y.Z` + `:vX.Y` + `:latest` aliases.

---

## Task overview

| # | Title | LOC ≈ |
|---|---|---|
| 1 | Throttle on /auth/password + integration test | 130 |
| 2 | Helm: NetworkPolicy template + values | 130 |
| 3 | Helm: PrometheusRule template + values | 180 |
| 4 | CI: release.yaml — multi-arch buildx + push on tag | 110 |
| 5 | e2e: ws-reconnect.spec.ts | 100 |
| 6 | Outcome doc | 0 |

Smaller than recent plans — 6 tasks, mostly mechanical.

---

## Task 1: Throttle /auth/password

**Files:**
- Modify: `crates/knot-server/src/routes/auth/local.rs`
- Modify: `crates/knot-server/tests/auth_password_integration.rs`

- [ ] **Step 1: Add throttle to handler**

Find the `change_password` handler (added in Plan 8 T1, commit `825938c`). Pattern after `login`:

1. Derive both throttle keys before reading the body:
   ```rust
   let ip_key = format!("pw:ip:{ip}");
   let user_key = format!("pw:user:{}", ctx.user_id);
   ```
   (`client_ip(&req)` helper already exists in this file — check + reuse.)

2. After validating the AuthContext but BEFORE the password reuse / weak checks:
   ```rust
   if matches!(state.throttle.check(&ip_key), Allow::No)
       || matches!(state.throttle.check(&user_key), Allow::No)
   {
       return json_err(StatusCode::TOO_MANY_REQUESTS, "auth.throttled", "too many attempts");
   }
   ```

3. Record failures on the existing error branches:
   - Wrong current password (401 invalid_credentials):
     ```rust
     state.throttle.record_failure(&ip_key);
     state.throttle.record_failure(&user_key);
     ```
   - User without `password_hash` (OIDC-only) — record IP only (no point penalizing a user for being OIDC).

4. Reset on success (before returning 204):
   ```rust
   state.throttle.reset(&ip_key);
   state.throttle.reset(&user_key);
   ```

- [ ] **Step 2: Add a test case**

Append to `crates/knot-server/tests/auth_password_integration.rs`:

```rust
#[tokio::test(flavor = "multi_thread")]
async fn throttle_returns_429_after_repeated_wrong_currents() {
    let (app, _pool) = build_app().await;
    let (sid, csrf) = setup_owner(&app).await;
    let csrf_token = csrf.trim_start_matches("csrf=").to_string();

    // 5 wrong-current attempts (CAPACITY = 5).
    for _ in 0..5 {
        let r = app.clone().oneshot(
            Request::post("/auth/password")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, format!("{sid}; {csrf}"))
                .header("X-CSRF-Token", &csrf_token)
                .body(Body::from(r#"{"current":"wrong","new":"correct-horse-22"}"#))
                .unwrap(),
        ).await.unwrap();
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    }

    // 6th attempt should be throttled.
    let r = app.oneshot(
        Request::post("/auth/password")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, format!("{sid}; {csrf}"))
            .header("X-CSRF-Token", &csrf_token)
            .body(Body::from(r#"{"current":"wrong","new":"correct-horse-22"}"#))
            .unwrap(),
    ).await.unwrap();
    assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    let body = axum::body::to_bytes(r.into_body(), 4096).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"]["code"], "auth.throttled");
}
```

> **Use whatever helpers the file already defines** (`build_app`, `setup_owner`, the existing CSRF extraction). The capacity constant in `knot-auth::throttle` is `5` — if it changes, adjust the loop count.

- [ ] **Step 3: Verify**

```bash
make compose.up
cargo nextest run -p knot-server --test auth_password_integration
cargo clippy -p knot-server --all-targets --all-features -- -D warnings
```

All 6 tests (5 existing + 1 new) pass; clippy clean.

- [ ] **Step 4: Commit**

```bash
git add crates/knot-server/
git commit -m "feat(knot-server): throttle /auth/password by user+ip"
```

---

## Task 2: Helm NetworkPolicy

**Files:**
- Modify: `deploy/helm/knot/values.yaml`
- Modify: `deploy/helm/knot/values.schema.json`
- Create: `deploy/helm/knot/templates/networkpolicy.yaml`

- [ ] **Step 1: values.yaml**

```yaml
networkPolicy:
  enabled: false
  # Selectors describing who is allowed to reach the http port.
  # Empty list = no http ingress (useful with a sidecar-only deployment).
  # The chart does NOT default to allowing everything — explicit is safer.
  ingressFrom:
    - namespaceSelector: {}     # any namespace
      podSelector: {}            # any pod
  # Selectors for who can scrape the metrics port. Default: same namespace.
  metricsIngressFrom:
    - podSelector:
        matchLabels:
          app.kubernetes.io/name: prometheus
```

- [ ] **Step 2: Template**

```yaml
{{- if .Values.networkPolicy.enabled -}}
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: {{ include "knot.fullname" . }}
  labels:
    {{- include "knot.labels" . | nindent 4 }}
spec:
  podSelector:
    matchLabels:
      {{- include "knot.selectorLabels" . | nindent 6 }}
  policyTypes:
    - Ingress
  ingress:
    {{- with .Values.networkPolicy.ingressFrom }}
    - from:
        {{- toYaml . | nindent 8 }}
      ports:
        - protocol: TCP
          port: 3000
    {{- end }}
    {{- if .Values.metrics.enabled }}
    {{- with .Values.networkPolicy.metricsIngressFrom }}
    - from:
        {{- toYaml . | nindent 8 }}
      ports:
        - protocol: TCP
          port: {{ $.Values.metrics.port }}
    {{- end }}
    {{- end }}
{{- end -}}
```

> **Note:** No egress policy. Adding one is one more values key but adds a real risk of breaking things (Postgres + Dex + OTLP + OIDC discovery all need egress). Out of scope; document in the chart README.

- [ ] **Step 3: values.schema.json**

```json
"networkPolicy": {
  "type": "object",
  "properties": {
    "enabled": { "type": "boolean" },
    "ingressFrom": { "type": "array" },
    "metricsIngressFrom": { "type": "array" }
  }
}
```

- [ ] **Step 4: Verify**

```bash
helm lint deploy/helm/knot --set database.url=x --set session.key=y --set networkPolicy.enabled=true
helm template knot deploy/helm/knot --set database.url=x --set session.key=y --set networkPolicy.enabled=true | grep -B 1 -A 15 NetworkPolicy
```

- [ ] **Step 5: Commit**

```bash
git add deploy/helm/
git commit -m "feat(deploy): optional default-deny NetworkPolicy template"
```

---

## Task 3: Helm PrometheusRule

**Files:**
- Modify: `deploy/helm/knot/values.yaml`
- Modify: `deploy/helm/knot/values.schema.json`
- Create: `deploy/helm/knot/templates/prometheusrule.yaml`

- [ ] **Step 1: values.yaml**

```yaml
alerting:
  enabled: false
  # Severity routed to whatever PrometheusRule labels your Alertmanager
  # config expects. Two tiers: fast burn = page, slow burn = ticket.
  pageSeverity: critical
  ticketSeverity: warning
  # Service-level objectives — match docs/SLO.md.
  # 99.5% availability over 30 days.
  errorBudgetTarget: 0.995
  # Latency target for /api/* (excluding health probes), in seconds.
  latencyP95Target: 0.25
```

- [ ] **Step 2: Template**

```yaml
{{- if and .Values.metrics.enabled .Values.serviceMonitor.enabled .Values.alerting.enabled -}}
{{- $budget := sub 1.0 .Values.alerting.errorBudgetTarget -}}
apiVersion: monitoring.coreos.com/v1
kind: PrometheusRule
metadata:
  name: {{ include "knot.fullname" . }}
  labels:
    {{- include "knot.labels" . | nindent 4 }}
spec:
  groups:
    - name: knot.availability
      rules:
        - alert: KnotErrorBudgetFastBurn
          expr: |
            (
              sum(rate(knot_http_requests_total{status_class="5xx",route!~"/api/health.*"}[1h]))
              /
              clamp_min(sum(rate(knot_http_requests_total{route!~"/api/health.*"}[1h])), 1)
            ) > (14.4 * {{ $budget }})
          for: 5m
          labels:
            severity: {{ .Values.alerting.pageSeverity | quote }}
          annotations:
            summary: "knot fast burn — 2% of 30d budget in 1h"
            description: "5xx rate is burning the error budget 14.4× faster than allowed."
        - alert: KnotErrorBudgetSlowBurn
          expr: |
            (
              sum(rate(knot_http_requests_total{status_class="5xx",route!~"/api/health.*"}[6h]))
              /
              clamp_min(sum(rate(knot_http_requests_total{route!~"/api/health.*"}[6h])), 1)
            ) > (6 * {{ $budget }})
          for: 30m
          labels:
            severity: {{ .Values.alerting.ticketSeverity | quote }}
          annotations:
            summary: "knot slow burn — 5% of 30d budget in 6h"
            description: "Sustained 5xx rate is burning the error budget 6× faster than allowed."
    - name: knot.latency
      rules:
        - alert: KnotLatencyP95High
          expr: |
            histogram_quantile(0.95,
              sum by (le) (
                rate(knot_http_request_duration_seconds_bucket{route!~"/api/health.*"}[5m])
              )
            ) > {{ .Values.alerting.latencyP95Target }}
          for: 30m
          labels:
            severity: {{ .Values.alerting.ticketSeverity | quote }}
          annotations:
            summary: "knot p95 latency above target"
            description: "P95 request latency has been above {{ .Values.alerting.latencyP95Target }}s for 30 minutes."
{{- end -}}
```

- [ ] **Step 3: values.schema.json**

```json
"alerting": {
  "type": "object",
  "properties": {
    "enabled": { "type": "boolean" },
    "pageSeverity": { "type": "string" },
    "ticketSeverity": { "type": "string" },
    "errorBudgetTarget": { "type": "number", "minimum": 0, "maximum": 1 },
    "latencyP95Target": { "type": "number", "minimum": 0 }
  }
}
```

- [ ] **Step 4: Verify**

```bash
helm lint deploy/helm/knot --set database.url=x --set session.key=y --set serviceMonitor.enabled=true --set alerting.enabled=true
helm template knot deploy/helm/knot --set database.url=x --set session.key=y --set serviceMonitor.enabled=true --set alerting.enabled=true | grep -A 25 PrometheusRule
```

- [ ] **Step 5: Commit**

```bash
git add deploy/helm/
git commit -m "feat(deploy): PrometheusRule template with SLO burn-rate alerts"
```

---

## Task 4: Release CI workflow

**Files:**
- Create: `.github/workflows/release.yaml`

- [ ] **Step 1: Write**

```yaml
name: Release

on:
  push:
    tags:
      - "v*"

permissions:
  contents: read
  packages: write

jobs:
  image:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Derive tags
        id: tags
        run: |
          REF="${GITHUB_REF_NAME}"        # e.g. v0.1.0
          MINOR="${REF%.*}"                # v0.1
          echo "version=$REF" >> "$GITHUB_OUTPUT"
          echo "minor=$MINOR" >> "$GITHUB_OUTPUT"

      - uses: docker/setup-qemu-action@v3
      - uses: docker/setup-buildx-action@v3

      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Build + push multi-arch
        uses: docker/build-push-action@v6
        with:
          context: .
          platforms: linux/amd64,linux/arm64
          push: true
          tags: |
            ghcr.io/${{ github.repository }}:${{ steps.tags.outputs.version }}
            ghcr.io/${{ github.repository }}:${{ steps.tags.outputs.minor }}
            ghcr.io/${{ github.repository }}:latest
          cache-from: type=gha
          cache-to: type=gha,mode=max

  chart:
    runs-on: ubuntu-latest
    needs: image
    steps:
      - uses: actions/checkout@v4
      - name: Set up Helm
        uses: azure/setup-helm@v4
        with:
          version: v3.16.0
      - name: Package
        run: |
          mkdir -p dist
          helm package deploy/helm/knot -d dist
          ls -la dist
      - name: Upload chart artifact
        uses: actions/upload-artifact@v4
        with:
          name: helm-chart
          path: dist/*.tgz
          retention-days: 90
```

> **Note:** Chart push to GHCR OCI is a small follow-up — for v0.1 we upload as a workflow artifact so a maintainer can fetch + push manually until the OCI cadence is set.

- [ ] **Step 2: Commit**

```bash
git add .github/
git commit -m "ci: release workflow — multi-arch image push + chart artifact on tag"
```

---

## Task 5: WS reconnect e2e

**Files:**
- Create: `e2e/flows/ws-reconnect.spec.ts`

- [ ] **Step 1: Write**

```ts
import { execSync } from "node:child_process";
import { expect, test } from "@playwright/test";

function reset() {
  const tables = ["acl_invalidations","audit_events","doc_markdown_cache","doc_snapshots","doc_updates","document_grants","documents","sessions","workspace_members","users","workspaces"].join(", ");
  execSync(`docker compose -f deploy/compose/dev.yml exec -T postgres psql -U knot -d knot -c "TRUNCATE TABLE ${tables} CASCADE"`, { cwd: "..", stdio: "pipe" });
}
test.beforeAll(reset);

test("editor reconnects after network flap; content preserved", async ({ page, context }) => {
  await page.goto("/setup");
  await page.getByTestId("setup-email").fill("o@e.com");
  await page.getByTestId("setup-display-name").fill("O");
  await page.getByTestId("setup-password").fill("owner-hunter22");
  await page.getByTestId("setup-submit").click();
  await page.getByTestId("new-doc").click();
  await page.waitForURL(/\/doc\/.+/);
  await expect(page.getByTestId("status-dot")).toHaveAttribute("data-status", "connected", { timeout: 10_000 });

  // Type something.
  const editor = page.locator("[data-testid='editor-host'] .ProseMirror");
  await editor.click();
  await page.keyboard.type("Before the flap.");

  // Drop the network. KnotProvider's onclose fires, status → offline.
  await context.setOffline(true);
  await expect(page.getByTestId("status-dot")).toHaveAttribute("data-status", "offline", { timeout: 5_000 });

  // Restore. KnotProvider's scheduleReconnect should reconnect within
  // its backoff (~500ms first attempt + jitter).
  await context.setOffline(false);
  await expect(page.getByTestId("status-dot")).toHaveAttribute("data-status", "connected", { timeout: 15_000 });

  // Content persists across reconnect (the Y.Doc never lost state).
  await expect(editor).toContainText("Before the flap.");
});
```

> **Note:** Playwright's `context.setOffline` blocks new connections but does NOT close existing sockets immediately. Most browsers + WS clients detect via subsequent ping/keepalive timeout, which can take 30 s+. If the test flaps because the offline transition isn't fast enough, an alternative is to use `route.abort()` on the `/collab` URL to force closure, or set offline + reload the page. Document this limitation if it surfaces.

- [ ] **Step 2: Run**

```bash
cd e2e
pnpm playwright test ws-reconnect.spec.ts
```

If it flaps, mark with `test.skip()` + a TODO and report.

Also run the full suite to confirm no regression:
```bash
pnpm playwright test
```

- [ ] **Step 3: Commit**

```bash
git add e2e/
git commit -m "test(e2e): WS reconnect after network flap"
```

---

## Task 6: Outcome doc

**Files:**
- Create: `docs/superpowers/research/2026-06-0X-plan12-outcome.md`
- Modify: `docs/superpowers/README.md` — add Plan 12 row

Use the same template as Plan 9/10/11 outcome docs:
- Status (GO / GO_WITH_CONCERNS / BLOCKED)
- Gates (cargo + helm + ci file validation + e2e)
- What landed
- What was non-obvious
- What's deferred (account lockout, cosign, helm chart OCI push, distroless)
- Carryforward — recommend Plan 13 (file uploads) or Plan 14 (full-text search) next

```bash
git add docs/
git commit -m "docs: Plan 12 outcome — production hardening"
```

---

## Self-review checklist

- [ ] `cargo test --workspace` green (+1 case in auth_password_integration)
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `pnpm playwright test` green — at least the 20 existing + ws-reconnect (skipped is OK if flaky and documented)
- [ ] `helm lint deploy/helm/knot --set ...networkPolicy.enabled=true --set alerting.enabled=true --set serviceMonitor.enabled=true` clean
- [ ] `helm template ... | kubectl apply --dry-run=client -f -` accepts the new NetworkPolicy (PrometheusRule fails dry-run on clusters without kube-prometheus-stack CRDs — expected)
- [ ] `.github/workflows/release.yaml` parses (yamllint or actionlint locally if available)
- [ ] No secret values in the workflow — only `${{ secrets.GITHUB_TOKEN }}`
- [ ] `docs/superpowers/README.md` updated with Plan 12 row
