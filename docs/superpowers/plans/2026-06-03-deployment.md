# Deployment Implementation Plan (Plan 9)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make knot installable on a stock Kubernetes cluster — a multi-arch (amd64 + arm64) static musl container image plus a Helm chart that covers Deployment, Service, Ingress, configuration, secrets, and database migrations, with a `helm test` pod and chart-testing CI gating future changes.

**Architecture:**
- **Image** — Single multi-stage `Dockerfile` with `cargo-zigbuild` for true cross-compilation (no QEMU per-arch emulation). Builder stage uses the host architecture (`$BUILDPLATFORM`); cross-compiles to `${TARGETARCH}-unknown-linux-musl`; copies the static binary into a `scratch` base. The Rust binary uses **mimalloc** as the global allocator (per project memory `feedback_mimalloc.md`).
- **Helm chart** — `deploy/helm/knot/` with one Deployment running `knot-server` + ClusterIP Service + optional Ingress. Configuration via ConfigMap + Secret. Database migrations run as a `pre-install` + `pre-upgrade` Helm Job so the new schema is in place before the new pods land. A `helm test` Pod exercises `/api/healthz`. **External Postgres + external Dex** assumed in production — no bundled subcharts.
- **CI** — A new GitHub Actions workflow runs `ct lint` + `ct install` in a kind cluster on every PR that touches `deploy/helm/`.

**Tech Stack:** Docker `buildx`, `cargo-zigbuild` (Zig-based linker for musl cross-compile), Helm 3, [chart-testing](https://github.com/helm/chart-testing) (`ct`), kind. No new server-side dependencies except the `mimalloc` Rust crate.

**Predecessors:**
- Plan 5 (CRDT + persistence, outcome at `docs/superpowers/research/2026-06-02-plan5-outcome.md`)
- Plan 6 (frontend, outcome at `docs/superpowers/research/2026-06-02-plan6-outcome.md`)
- Plan 8 (auth completion, outcome at `docs/superpowers/research/2026-06-03-plan8-outcome.md`, HEAD `9fca81f`)

**Spec coverage:**

| Spec section | Tasks |
|---|---|
| §11.1 Container image — multi-arch + small | T1, T2 |
| §11.2 Helm chart — Deployment/Service/Ingress/Config/Secret/Job | T4–T9 |
| §11.3 `helm test` smoke pod | T10 |
| §11.4 CI gate (chart-testing in kind) | T11 |
| §11.5 First-run runbook | T12 |

**Out of scope** (intentionally deferred):

- **Frontend image** — for v0.1 the SPA is served by `knot-server` (Plan 1 + Plan 6 ship `web/dist` as a static asset behind the same router). If a separate `nginx` frontend image is desired later, that's a follow-up.

  *If* `knot-server` does NOT currently embed `web/dist`, T2 must add a `pnpm build` stage and serve it from the binary; check before assuming.
- **Bundled Postgres + Dex subcharts** — `values.yaml` documents the external endpoints required. Adding `postgresql` / `dex` as conditional dependencies is a follow-up plan.
- **Image signing (cosign)** — defer to a hardening plan.
- **HPA, PodDisruptionBudget, NetworkPolicy** — defer to a separate scaling/HA plan.
- **ServiceMonitor for Prometheus** — defer to Plan 10 (observability).
- **Multi-arch CI image push** — local `docker buildx` is wired by T2; pushing on tag is a small follow-up that can be added directly to the existing workflow.

---

## File map

```
knot/
├── Dockerfile                                  (new) multi-stage, cargo-zigbuild, mimalloc, scratch
├── .dockerignore                               (new)
├── crates/knot-server/Cargo.toml               (modify) add mimalloc dep
├── crates/knot-server/src/main.rs              (modify) #[global_allocator] = MiMalloc
├── Makefile                                    (modify) +image.build / image.smoke targets
│
├── deploy/helm/knot/
│   ├── Chart.yaml                              (new)
│   ├── values.yaml                             (new) documented external Postgres/Dex
│   ├── values.schema.json                      (new) optional but nice — validates values
│   ├── .helmignore                             (new)
│   ├── README.md                               (new) install runbook
│   └── templates/
│       ├── _helpers.tpl                        (new)
│       ├── deployment.yaml                     (new)
│       ├── service.yaml                        (new)
│       ├── ingress.yaml                        (new)
│       ├── configmap.yaml                      (new)
│       ├── secret.yaml                         (new) created only when .Values.auth.create=true
│       ├── serviceaccount.yaml                 (new)
│       ├── migrate-job.yaml                    (new) pre-install + pre-upgrade hook
│       └── tests/
│           └── healthz-test.yaml               (new) helm test pod
│
└── .github/workflows/
    └── helm-ci.yaml                            (new) ct lint + ct install in kind
```

---

## Conventions

- **Versioning** — chart `appVersion` mirrors the git tag (`v0.1.0` etc.). Chart `version` follows semver independently. Both start at `0.1.0`.
- **Naming** — resources use Helm's standard `{{ include "knot.fullname" . }}` pattern. The shared label set comes from `_helpers.tpl`.
- **External secrets** — the chart can either create a Secret from `values.yaml` (dev convenience, `auth.create=true`) or reference an `existingSecretName` (production). Both paths supported.
- **Health probes** — `readiness` on `/api/readyz`, `liveness` on `/api/healthz`. Existing routes from `crates/knot-server/src/routes/health.rs`.
- **Env var contract** — exactly the names knot-server already reads via figment (`KNOT_*`). The plan keeps them flat — no Helm-only aliases that translate at template-time.
- **Migration job** — uses the same image as the Deployment with `command: ["/knot-server", "migrate"]`. **If knot-server does NOT currently have a `migrate` subcommand**, T2 must add one (or use a one-shot `KNOT_MIGRATE_ONLY=true` env flag that runs migrations and exits). Verify first.

---

## Task overview

| # | Title | LOC ≈ |
|---|---|---|
| 1 | Add mimalloc global allocator + `migrate` subcommand if missing | 60 |
| 2 | Multi-arch Dockerfile (cargo-zigbuild, musl, scratch) | 80 |
| 3 | Local image smoke test (build + run against dev-compose Postgres) | research |
| 4 | Helm chart skeleton (Chart.yaml + values.yaml + _helpers.tpl + .helmignore) | 180 |
| 5 | Deployment + Service templates | 160 |
| 6 | ConfigMap + Secret templates | 140 |
| 7 | Ingress + ServiceAccount templates | 100 |
| 8 | Migration Job (pre-install + pre-upgrade hook) | 90 |
| 9 | Helm test pod (healthz probe) | 50 |
| 10 | values.schema.json | 120 |
| 11 | chart-testing CI workflow | 120 |
| 12 | README install runbook + outcome doc | 0 |

---

## Task 1: mimalloc global allocator + migrate subcommand

**Files:**
- Modify: `crates/knot-server/Cargo.toml`
- Modify: `crates/knot-server/src/main.rs`

- [ ] **Step 1: Verify whether a `migrate` subcommand exists**

```bash
grep -n "migrate\|subcommand\|clap::Parser" crates/knot-server/src/main.rs crates/knot-server/Cargo.toml | head -20
```

If `knot-server migrate` already exists → skip Step 4.

- [ ] **Step 2: Add mimalloc**

Edit `crates/knot-server/Cargo.toml`. Under `[dependencies]`, add:

```toml
mimalloc = { version = "0.1", default-features = false }
```

`default-features = false` skips the optional secure-mode and override extras that we don't need for musl static linking.

- [ ] **Step 3: Wire it as the global allocator**

Edit `crates/knot-server/src/main.rs`. At the top (before `fn main`):

```rust
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
```

- [ ] **Step 4 (if needed): Add a `migrate` subcommand**

If `main.rs` does NOT take subcommands today, add the smallest possible thing that runs migrations and exits. Use whatever arg parser the codebase already pulls in (likely `clap`). Example:

```rust
let args: Vec<String> = std::env::args().collect();
if args.iter().any(|a| a == "migrate") {
    // load config → connect to Postgres → run sqlx::migrate!() → exit 0
    let cfg = knot_config::Config::load().expect("config");
    let pool = sqlx::PgPool::connect(&cfg.database_url).await.expect("connect");
    sqlx::migrate!("../../migrations").run(&pool).await.expect("migrate");
    return;
}
// ...existing main body...
```

(Adapt to the actual `main` signature — async runtime, error type.)

- [ ] **Step 5: Verify**

```bash
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

All clean.

- [ ] **Step 6: Commit**

```bash
git add crates/knot-server/
git commit -m "feat(knot-server): mimalloc global allocator + migrate subcommand"
```

---

## Task 2: Multi-arch Dockerfile

**Files:**
- Create: `Dockerfile`
- Create: `.dockerignore`
- Modify: `Makefile`

- [ ] **Step 1: Confirm the SPA build story**

Check whether `knot-server` embeds `web/dist`:

```bash
grep -rn "rust-embed\|include_dir!\|ServeDir\|web/dist" crates/knot-server/src/ | head -10
```

- If it embeds via `rust-embed` / `include_dir!`: the SPA build must happen IN the Dockerfile before the Rust build.
- If it uses `tower-http::ServeDir` pointing to `./web/dist`: copy `web/dist` into the runtime stage and set `WORKDIR` appropriately.
- If it does NOT serve the SPA at all today: the chart's `Ingress` (T7) can split paths; document this in `values.yaml`.

Capture the finding in a one-line comment at the top of the Dockerfile so future readers know.

- [ ] **Step 2: Write the Dockerfile**

Create `/home/nik/Development/knot/Dockerfile`:

```dockerfile
# syntax=docker/dockerfile:1.7
# Multi-stage build for knot-server.
# - SPA build (T6 Plan 6 architecture): pnpm build → web/dist
# - Rust build: cargo-zigbuild cross-compiles to ${TARGETARCH}-unknown-linux-musl
#   using the BUILDPLATFORM's host toolchain (no QEMU per-arch).
# - Runtime: scratch + the static binary + CA certs.

# ----- Web SPA build -----
FROM --platform=$BUILDPLATFORM node:20-alpine AS web-builder
WORKDIR /app/web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ .
RUN pnpm build
# Output: /app/web/dist

# ----- Rust build -----
FROM --platform=$BUILDPLATFORM rust:1.83-alpine AS rust-builder
ARG TARGETARCH
RUN apk add --no-cache musl-dev openssl-dev pkgconf clang lld build-base curl
RUN cargo install cargo-zigbuild --locked
# Install zig (the linker cargo-zigbuild drives)
RUN curl -sSL https://ziglang.org/download/0.13.0/zig-linux-$(uname -m)-0.13.0.tar.xz \
    | tar -xJ -C /usr/local && ln -s /usr/local/zig-linux-*/zig /usr/local/bin/zig

# Map Docker arch names to Rust target triples.
RUN case "$TARGETARCH" in \
      amd64) echo x86_64-unknown-linux-musl > /target ;; \
      arm64) echo aarch64-unknown-linux-musl > /target ;; \
      *) echo "unsupported arch: $TARGETARCH" >&2; exit 1 ;; \
    esac
RUN rustup target add "$(cat /target)"

WORKDIR /src
# Cache deps separately from sources.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
COPY tools ./tools
# Bring in the freshly-built SPA so any embed crate sees it.
COPY --from=web-builder /app/web/dist ./web/dist

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo zigbuild --release --target "$(cat /target)" --bin knot-server && \
    cp "target/$(cat /target)/release/knot-server" /knot-server

# ----- Runtime: scratch + binary + certs -----
FROM scratch AS runtime
COPY --from=rust-builder /knot-server /knot-server
# Static musl + mimalloc: the binary brings its own libc + allocator.
# We still need CA certs for TLS-bound features (OIDC discovery, OTLP, etc.).
COPY --from=rust-builder /etc/ssl/cert.pem /etc/ssl/cert.pem
USER 65534:65534
EXPOSE 3000
ENV KNOT_LOG_FORMAT=json
ENTRYPOINT ["/knot-server"]
```

> **If** the SPA is served by a separate frontend image, drop the `web-builder` stage and the `COPY --from=web-builder` line. Plan note from T19 says the spike serves SPA via `knot-server` after the Plan 6 rewrite, but verify Step 1 before deleting either path.

- [ ] **Step 3: Write `.dockerignore`**

```text
target/
**/target/
node_modules/
**/node_modules/
.git/
.github/
docs/
e2e/
*.md
deploy/compose/
**/.DS_Store
**/dist/
```

(The web SPA build runs INSIDE the image, so excluding `web/dist` is safe — it'll be regenerated.)

- [ ] **Step 4: Add Makefile targets**

Edit `/home/nik/Development/knot/Makefile`. Add:

```makefile
IMAGE_NAME ?= knot
IMAGE_TAG  ?= dev

.PHONY: image.build
image.build: ## build multi-arch image locally (requires docker buildx)
	docker buildx build \
	  --platform linux/amd64,linux/arm64 \
	  --tag $(IMAGE_NAME):$(IMAGE_TAG) \
	  --load \
	  .

.PHONY: image.build.host
image.build.host: ## build single-arch image for the host (faster, for smoke testing)
	docker build --tag $(IMAGE_NAME):$(IMAGE_TAG) .

.PHONY: image.smoke
image.smoke: image.build.host ## run a freshly-built image against dev-compose Postgres
	docker run --rm --network host \
	  -e KNOT_DATABASE_URL="postgres://knot:knot@localhost:5432/knot" \
	  -e KNOT_SESSION_KEY="test-key-32-bytes-aaaaaaaaaaaaaa" \
	  $(IMAGE_NAME):$(IMAGE_TAG)
```

- [ ] **Step 5: Verify the single-arch build works**

```bash
make compose.up    # need Postgres for the migrate run below
make image.build.host
docker images $(IMAGE_NAME):$(IMAGE_TAG)
```

Expected: image exists and is < 50 MB (target ~15–30 MB on scratch).

- [ ] **Step 6: Commit**

```bash
git add Dockerfile .dockerignore Makefile
git commit -m "build: multi-arch musl Dockerfile + image targets"
```

---

## Task 3: Local image smoke test

**Files:** none (research) — optional commit with any fixes uncovered.

- [ ] **Step 1: Boot the image against dev Postgres**

```bash
docker run --rm --network host \
  -e KNOT_DATABASE_URL="postgres://knot:knot@localhost:5432/knot" \
  -e KNOT_SESSION_KEY="test-key-32-bytes-aaaaaaaaaaaaaa" \
  knot:dev migrate

docker run --rm --network host -d --name knot-smoke \
  -e KNOT_DATABASE_URL="postgres://knot:knot@localhost:5432/knot" \
  -e KNOT_SESSION_KEY="test-key-32-bytes-aaaaaaaaaaaaaa" \
  knot:dev

sleep 2
curl -s http://localhost:3000/api/healthz
docker stop knot-smoke
```

Expected: `migrate` exits 0; the runtime container starts; `/api/healthz` returns 200.

- [ ] **Step 2: Multi-arch build (optional)**

```bash
docker buildx create --use --name knot-builder
make image.build
```

This confirms cargo-zigbuild works for both arches without QEMU. Failure usually means a missing arch-specific dep — fix in Step 3 of Task 2.

- [ ] **Step 3: Document & commit any fixes**

If Step 1 or 2 surfaced a bug in Task 1/2 work, fix and commit with `fix:` prefix.

---

## Task 4: Helm chart skeleton

**Files:**
- Create: `deploy/helm/knot/Chart.yaml`
- Create: `deploy/helm/knot/values.yaml`
- Create: `deploy/helm/knot/.helmignore`
- Create: `deploy/helm/knot/templates/_helpers.tpl`

- [ ] **Step 1: `Chart.yaml`**

```yaml
apiVersion: v2
name: knot
description: |
  knot — self-hosted Confluence/Notion alternative.
  Multi-user collaborative knowledge base with CRDT-backed
  real-time editing.
type: application
version: 0.1.0
appVersion: "0.1.0"
home: https://github.com/voss/knot
sources:
  - https://github.com/voss/knot
maintainers:
  - name: Niklas Voss
icon: ""
keywords:
  - knot
  - wiki
  - collaboration
```

- [ ] **Step 2: `values.yaml`**

```yaml
# Default values for knot.
# This chart assumes an EXTERNAL Postgres (with the pgvector / no extra extensions
# beyond Postgres 16 features) and an EXTERNAL OIDC provider (e.g. Dex, Keycloak,
# Auth0). The dev-compose stack in deploy/compose is for local development only.

image:
  repository: ghcr.io/voss/knot
  tag: ""              # defaults to .Chart.AppVersion at template-render time
  pullPolicy: IfNotPresent
  pullSecrets: []

replicaCount: 1

# Required external endpoints.
database:
  # Connection URL. Set via existingSecret if you'd rather not put it in values.yaml.
  url: ""
  existingSecretName: ""
  existingSecretKey: "url"

# Session signing key. 32 bytes, base64 or raw.
session:
  key: ""
  existingSecretName: ""
  existingSecretKey: "key"

# OIDC configuration (optional).
oidc:
  enabled: false
  issuer: ""
  clientId: ""
  clientSecret: ""
  redirectUrl: ""              # https://<host>/auth/oidc/callback
  existingSecretName: ""        # secret with: client_secret
  autoProvision: "off"          # off | always | domain | group
  allowedDomains: ""
  roleFromGroups: ""            # JSON map: {"engineers":"editor", ...}

baseUrl: ""                     # e.g. https://knot.example.com (used in OIDC redirects)

logLevel: info
logFormat: json

service:
  type: ClusterIP
  port: 80
  targetPort: 3000

ingress:
  enabled: false
  className: ""
  annotations: {}
  hosts:
    - host: knot.example.com
      paths:
        - path: /
          pathType: Prefix
  tls: []

resources:
  requests:
    cpu: 100m
    memory: 128Mi
  limits:
    cpu: 1000m
    memory: 512Mi

podAnnotations: {}
podLabels: {}
podSecurityContext:
  runAsNonRoot: true
  runAsUser: 65534
  runAsGroup: 65534
  fsGroup: 65534
securityContext:
  allowPrivilegeEscalation: false
  capabilities:
    drop:
      - ALL
  readOnlyRootFilesystem: true

serviceAccount:
  create: true
  name: ""
  annotations: {}

# Migration job: runs as a Helm hook before install + upgrade.
migrations:
  enabled: true
  backoffLimit: 3

# Liveness / readiness probes on /api/healthz + /api/readyz.
probes:
  enabled: true

nodeSelector: {}
tolerations: []
affinity: {}
```

- [ ] **Step 3: `.helmignore`**

```text
.DS_Store
.git/
.gitignore
.vscode/
.idea/
*.tmproj
*.swp
*.bak
*.tmp
*.orig
.*.un~
```

- [ ] **Step 4: `_helpers.tpl`**

Use the standard helper set Helm scaffolds. Quick paste:

```tpl
{{/*
Common labels & selectors.
*/}}
{{- define "knot.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "knot.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := default .Chart.Name .Values.nameOverride -}}
{{- if contains $name .Release.Name -}}
{{- .Release.Name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "knot.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
app.kubernetes.io/name: {{ include "knot.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{- define "knot.selectorLabels" -}}
app.kubernetes.io/name: {{ include "knot.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{- define "knot.serviceAccountName" -}}
{{- if .Values.serviceAccount.create -}}
{{- default (include "knot.fullname" .) .Values.serviceAccount.name -}}
{{- else -}}
{{- default "default" .Values.serviceAccount.name -}}
{{- end -}}
{{- end -}}

{{/*
The Secret name we read env from at runtime.
Either user-provided (existingSecretName) or rendered by this chart (templates/secret.yaml).
*/}}
{{- define "knot.secretName" -}}
{{- if .Values.database.existingSecretName -}}{{ .Values.database.existingSecretName }}{{- else -}}{{ include "knot.fullname" . }}{{- end -}}
{{- end -}}
```

- [ ] **Step 5: Lint**

```bash
helm lint deploy/helm/knot
```

Expected: 0 errors (will warn about empty `templates/` directory until Tasks 5–9 add resources).

- [ ] **Step 6: Commit**

```bash
git add deploy/helm/
git commit -m "feat(deploy): Helm chart skeleton (Chart.yaml + values.yaml + helpers)"
```

---

## Task 5: Deployment + Service templates

**Files:**
- Create: `deploy/helm/knot/templates/deployment.yaml`
- Create: `deploy/helm/knot/templates/service.yaml`

- [ ] **Step 1: `deployment.yaml`**

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ include "knot.fullname" . }}
  labels:
    {{- include "knot.labels" . | nindent 4 }}
spec:
  replicas: {{ .Values.replicaCount }}
  selector:
    matchLabels:
      {{- include "knot.selectorLabels" . | nindent 6 }}
  template:
    metadata:
      annotations:
        {{- with .Values.podAnnotations }}
        {{- toYaml . | nindent 8 }}
        {{- end }}
        checksum/config: {{ include (print $.Template.BasePath "/configmap.yaml") . | sha256sum }}
      labels:
        {{- include "knot.selectorLabels" . | nindent 8 }}
        {{- with .Values.podLabels }}
        {{- toYaml . | nindent 8 }}
        {{- end }}
    spec:
      serviceAccountName: {{ include "knot.serviceAccountName" . }}
      securityContext:
        {{- toYaml .Values.podSecurityContext | nindent 8 }}
      {{- with .Values.image.pullSecrets }}
      imagePullSecrets:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      containers:
        - name: knot
          image: "{{ .Values.image.repository }}:{{ .Values.image.tag | default .Chart.AppVersion }}"
          imagePullPolicy: {{ .Values.image.pullPolicy }}
          securityContext:
            {{- toYaml .Values.securityContext | nindent 12 }}
          ports:
            - name: http
              containerPort: 3000
              protocol: TCP
          envFrom:
            - configMapRef:
                name: {{ include "knot.fullname" . }}
            - secretRef:
                name: {{ include "knot.secretName" . }}
          {{- if .Values.probes.enabled }}
          livenessProbe:
            httpGet:
              path: /api/healthz
              port: http
            initialDelaySeconds: 5
            periodSeconds: 10
          readinessProbe:
            httpGet:
              path: /api/readyz
              port: http
            initialDelaySeconds: 2
            periodSeconds: 5
          {{- end }}
          resources:
            {{- toYaml .Values.resources | nindent 12 }}
      {{- with .Values.nodeSelector }}
      nodeSelector:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .Values.affinity }}
      affinity:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .Values.tolerations }}
      tolerations:
        {{- toYaml . | nindent 8 }}
      {{- end }}
```

- [ ] **Step 2: `service.yaml`**

```yaml
apiVersion: v1
kind: Service
metadata:
  name: {{ include "knot.fullname" . }}
  labels:
    {{- include "knot.labels" . | nindent 4 }}
spec:
  type: {{ .Values.service.type }}
  ports:
    - port: {{ .Values.service.port }}
      targetPort: {{ .Values.service.targetPort }}
      protocol: TCP
      name: http
  selector:
    {{- include "knot.selectorLabels" . | nindent 4 }}
```

- [ ] **Step 3: Render-test**

```bash
helm template knot deploy/helm/knot --set database.url=postgres://x --set session.key=aaaaaaaa | head -60
helm lint deploy/helm/knot --set database.url=postgres://x --set session.key=aaaaaaaa
```

Expected: clean output, no missing values errors.

- [ ] **Step 4: Commit**

```bash
git add deploy/helm/
git commit -m "feat(deploy): Deployment + Service templates"
```

---

## Task 6: ConfigMap + Secret templates

**Files:**
- Create: `deploy/helm/knot/templates/configmap.yaml`
- Create: `deploy/helm/knot/templates/secret.yaml`

- [ ] **Step 1: `configmap.yaml`**

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ include "knot.fullname" . }}
  labels:
    {{- include "knot.labels" . | nindent 4 }}
data:
  KNOT_LOG_LEVEL: {{ .Values.logLevel | quote }}
  KNOT_LOG_FORMAT: {{ .Values.logFormat | quote }}
  KNOT_BASE_URL: {{ .Values.baseUrl | quote }}
  {{- if .Values.oidc.enabled }}
  KNOT_OIDC_ENABLED: "true"
  KNOT_OIDC_ISSUER: {{ .Values.oidc.issuer | quote }}
  KNOT_OIDC_CLIENT_ID: {{ .Values.oidc.clientId | quote }}
  KNOT_OIDC_REDIRECT_URL: {{ .Values.oidc.redirectUrl | quote }}
  KNOT_OIDC_AUTO_PROVISION: {{ .Values.oidc.autoProvision | quote }}
  {{- with .Values.oidc.allowedDomains }}
  KNOT_OIDC_ALLOWED_DOMAINS: {{ . | quote }}
  {{- end }}
  {{- with .Values.oidc.roleFromGroups }}
  KNOT_OIDC_ROLE_FROM_GROUPS: {{ . | quote }}
  {{- end }}
  {{- end }}
```

- [ ] **Step 2: `secret.yaml`**

This template renders **only when no external secret is referenced**. When the user supplies `database.existingSecretName`, they take responsibility for providing all expected keys.

```yaml
{{- if and (not .Values.database.existingSecretName) (not .Values.session.existingSecretName) -}}
apiVersion: v1
kind: Secret
metadata:
  name: {{ include "knot.fullname" . }}
  labels:
    {{- include "knot.labels" . | nindent 4 }}
type: Opaque
stringData:
  KNOT_DATABASE_URL: {{ required "database.url or database.existingSecretName is required" .Values.database.url | quote }}
  KNOT_SESSION_KEY: {{ required "session.key or session.existingSecretName is required" .Values.session.key | quote }}
  {{- if and .Values.oidc.enabled .Values.oidc.clientSecret }}
  KNOT_OIDC_CLIENT_SECRET: {{ .Values.oidc.clientSecret | quote }}
  {{- end }}
{{- end -}}
```

- [ ] **Step 3: Render & lint**

```bash
helm template knot deploy/helm/knot \
  --set database.url=postgres://knot:knot@db/knot \
  --set session.key=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  | grep -A 2 KNOT_
```

Expected: ConfigMap + Secret blocks present with the values.

- [ ] **Step 4: Commit**

```bash
git add deploy/helm/
git commit -m "feat(deploy): ConfigMap + Secret templates"
```

---

## Task 7: Ingress + ServiceAccount templates

**Files:**
- Create: `deploy/helm/knot/templates/ingress.yaml`
- Create: `deploy/helm/knot/templates/serviceaccount.yaml`

- [ ] **Step 1: `ingress.yaml`**

```yaml
{{- if .Values.ingress.enabled -}}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {{ include "knot.fullname" . }}
  labels:
    {{- include "knot.labels" . | nindent 4 }}
  {{- with .Values.ingress.annotations }}
  annotations:
    {{- toYaml . | nindent 4 }}
  {{- end }}
spec:
  {{- with .Values.ingress.className }}
  ingressClassName: {{ . }}
  {{- end }}
  {{- with .Values.ingress.tls }}
  tls:
    {{- toYaml . | nindent 4 }}
  {{- end }}
  rules:
    {{- range .Values.ingress.hosts }}
    - host: {{ .host | quote }}
      http:
        paths:
          {{- range .paths }}
          - path: {{ .path }}
            pathType: {{ .pathType }}
            backend:
              service:
                name: {{ include "knot.fullname" $ }}
                port:
                  number: {{ $.Values.service.port }}
          {{- end }}
    {{- end }}
{{- end -}}
```

- [ ] **Step 2: `serviceaccount.yaml`**

```yaml
{{- if .Values.serviceAccount.create -}}
apiVersion: v1
kind: ServiceAccount
metadata:
  name: {{ include "knot.serviceAccountName" . }}
  labels:
    {{- include "knot.labels" . | nindent 4 }}
  {{- with .Values.serviceAccount.annotations }}
  annotations:
    {{- toYaml . | nindent 4 }}
  {{- end }}
{{- end -}}
```

- [ ] **Step 3: Commit**

```bash
helm lint deploy/helm/knot --set database.url=x --set session.key=y --set ingress.enabled=true
git add deploy/helm/
git commit -m "feat(deploy): Ingress + ServiceAccount templates"
```

---

## Task 8: Migration Job

**Files:**
- Create: `deploy/helm/knot/templates/migrate-job.yaml`

- [ ] **Step 1: Write the hook**

```yaml
{{- if .Values.migrations.enabled -}}
apiVersion: batch/v1
kind: Job
metadata:
  name: {{ include "knot.fullname" . }}-migrate
  labels:
    {{- include "knot.labels" . | nindent 4 }}
  annotations:
    "helm.sh/hook": pre-install,pre-upgrade
    "helm.sh/hook-weight": "-5"
    "helm.sh/hook-delete-policy": before-hook-creation
spec:
  backoffLimit: {{ .Values.migrations.backoffLimit }}
  ttlSecondsAfterFinished: 600
  template:
    metadata:
      labels:
        {{- include "knot.selectorLabels" . | nindent 8 }}
        app.kubernetes.io/component: migrate
    spec:
      restartPolicy: OnFailure
      serviceAccountName: {{ include "knot.serviceAccountName" . }}
      securityContext:
        {{- toYaml .Values.podSecurityContext | nindent 8 }}
      containers:
        - name: migrate
          image: "{{ .Values.image.repository }}:{{ .Values.image.tag | default .Chart.AppVersion }}"
          imagePullPolicy: {{ .Values.image.pullPolicy }}
          command: ["/knot-server", "migrate"]
          envFrom:
            - configMapRef:
                name: {{ include "knot.fullname" . }}
            - secretRef:
                name: {{ include "knot.secretName" . }}
          securityContext:
            {{- toYaml .Values.securityContext | nindent 12 }}
{{- end -}}
```

- [ ] **Step 2: Commit**

```bash
git add deploy/helm/
git commit -m "feat(deploy): pre-install/upgrade migration Job"
```

---

## Task 9: Helm test pod

**Files:**
- Create: `deploy/helm/knot/templates/tests/healthz-test.yaml`

- [ ] **Step 1: Write the test**

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: "{{ include "knot.fullname" . }}-test-healthz"
  labels:
    {{- include "knot.labels" . | nindent 4 }}
  annotations:
    "helm.sh/hook": test
spec:
  restartPolicy: Never
  containers:
    - name: curl
      image: curlimages/curl:8.10.1
      command:
        - sh
        - -c
        - |
          set -eu
          for i in 1 2 3 4 5 6 7 8 9 10; do
            if curl -fsSL --max-time 5 "http://{{ include "knot.fullname" . }}:{{ .Values.service.port }}/api/healthz" >/dev/null; then
              echo "healthz OK"
              exit 0
            fi
            echo "attempt $i: healthz not ready"
            sleep 2
          done
          echo "healthz never came up" >&2
          exit 1
```

- [ ] **Step 2: Commit**

```bash
git add deploy/helm/
git commit -m "feat(deploy): helm test pod hitting /api/healthz"
```

---

## Task 10: values.schema.json

**Files:**
- Create: `deploy/helm/knot/values.schema.json`

- [ ] **Step 1: Write the schema**

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "knot Helm values",
  "type": "object",
  "required": ["image", "database", "session"],
  "properties": {
    "image": {
      "type": "object",
      "required": ["repository"],
      "properties": {
        "repository": { "type": "string", "minLength": 1 },
        "tag": { "type": "string" },
        "pullPolicy": { "enum": ["Always", "IfNotPresent", "Never"] },
        "pullSecrets": { "type": "array" }
      }
    },
    "replicaCount": { "type": "integer", "minimum": 1 },
    "database": {
      "type": "object",
      "properties": {
        "url": { "type": "string" },
        "existingSecretName": { "type": "string" },
        "existingSecretKey": { "type": "string" }
      },
      "anyOf": [
        { "required": ["url"] },
        { "required": ["existingSecretName"] }
      ]
    },
    "session": {
      "type": "object",
      "properties": {
        "key": { "type": "string" },
        "existingSecretName": { "type": "string" },
        "existingSecretKey": { "type": "string" }
      },
      "anyOf": [
        { "required": ["key"] },
        { "required": ["existingSecretName"] }
      ]
    },
    "oidc": {
      "type": "object",
      "properties": {
        "enabled": { "type": "boolean" },
        "issuer": { "type": "string" },
        "clientId": { "type": "string" },
        "clientSecret": { "type": "string" },
        "redirectUrl": { "type": "string" },
        "autoProvision": { "enum": ["off", "always", "domain", "group"] },
        "allowedDomains": { "type": "string" },
        "roleFromGroups": { "type": "string" }
      }
    },
    "baseUrl": { "type": "string" },
    "logLevel": { "enum": ["trace", "debug", "info", "warn", "error"] },
    "logFormat": { "enum": ["json", "pretty"] },
    "service": {
      "type": "object",
      "properties": {
        "type": { "enum": ["ClusterIP", "NodePort", "LoadBalancer"] },
        "port": { "type": "integer" },
        "targetPort": { "type": "integer" }
      }
    },
    "ingress": {
      "type": "object",
      "properties": {
        "enabled": { "type": "boolean" }
      }
    },
    "migrations": {
      "type": "object",
      "properties": {
        "enabled": { "type": "boolean" },
        "backoffLimit": { "type": "integer", "minimum": 0 }
      }
    },
    "probes": {
      "type": "object",
      "properties": {
        "enabled": { "type": "boolean" }
      }
    }
  }
}
```

- [ ] **Step 2: Verify it catches missing values**

```bash
helm template knot deploy/helm/knot 2>&1 | grep -i "validation" || echo "would render"
helm template knot deploy/helm/knot --set database.url=x --set session.key=y >/dev/null && echo OK
```

The first should complain (missing required), the second should succeed.

- [ ] **Step 3: Commit**

```bash
git add deploy/helm/
git commit -m "feat(deploy): values.schema.json — validate required inputs"
```

---

## Task 11: chart-testing CI workflow

**Files:**
- Create: `.github/workflows/helm-ci.yaml`

- [ ] **Step 1: Write the workflow**

```yaml
name: Helm chart CI

on:
  pull_request:
    paths:
      - "deploy/helm/**"
      - ".github/workflows/helm-ci.yaml"
  push:
    branches: [main, master]
    paths:
      - "deploy/helm/**"

jobs:
  lint-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Set up Helm
        uses: azure/setup-helm@v4
        with:
          version: v3.16.0

      - name: Set up chart-testing
        uses: helm/chart-testing-action@v2.7.0

      - name: ct lint
        run: ct lint --target-branch main --chart-dirs deploy/helm

      - name: Create kind cluster
        uses: helm/kind-action@v1.10.0
        with:
          version: v0.24.0
          wait: 60s

      - name: Seed required dummy values
        run: |
          mkdir -p deploy/helm/knot/ci
          cat > deploy/helm/knot/ci/test-values.yaml <<EOF
          database:
            url: "postgres://knot:knot@127.0.0.1:5432/knot"
          session:
            key: "test-key-32-bytes-aaaaaaaaaaaaaa"
          migrations:
            enabled: false  # no Postgres in CI cluster
          probes:
            enabled: false  # binary will be unable to reach DB; skip probes
          EOF

      - name: ct install
        run: ct install --target-branch main --chart-dirs deploy/helm
```

> **Note:** Without a Postgres in the CI kind cluster, the pod's probes will fail. The `test-values.yaml` disables them so `ct install` is validating the **chart correctness** (rendered + applied + scheduled), not runtime behavior. A follow-up plan can add a Postgres sidecar for true install-from-zero tests.

- [ ] **Step 2: Commit**

```bash
git add .github/
git commit -m "ci: helm chart-testing (ct lint + ct install in kind)"
```

---

## Task 12: README install runbook + outcome doc

**Files:**
- Create: `deploy/helm/knot/README.md`
- Create: `docs/superpowers/research/2026-06-0X-plan9-outcome.md`

- [ ] **Step 1: Chart README**

Cover at minimum:
- Prerequisites (Postgres 16; optional OIDC IdP; ingress controller; cert-manager if TLS).
- `helm install` example with the minimal required values.
- How to point at an existing Secret instead of inline `database.url` / `session.key`.
- OIDC configuration snippet for Dex + Keycloak.
- Upgrade path notes (the `pre-upgrade` migration Job).

- [ ] **Step 2: Outcome doc**

Use the same template as the Plan 6 / Plan 8 outcome docs.

- [ ] **Step 3: Commit**

```bash
git add deploy/ docs/
git commit -m "docs(deploy): chart README + Plan 9 outcome"
```

---

## Self-review checklist

- [ ] `cargo test --workspace` green
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `pnpm tsc`, `pnpm lint`, `pnpm test`, `pnpm playwright test` green
- [ ] `make image.build.host` produces a < 50 MB image
- [ ] `make image.smoke` exits cleanly after `curl /api/healthz` returns 200
- [ ] `helm lint deploy/helm/knot` clean (with `--set database.url=x --set session.key=y`)
- [ ] `helm template ... | kubectl apply --dry-run=client -f -` accepts all manifests
- [ ] `ct lint --chart-dirs deploy/helm` passes locally
- [ ] Image manifest declares both `linux/amd64` and `linux/arm64`
- [ ] No secret data committed (audit `git diff` before pushing)
