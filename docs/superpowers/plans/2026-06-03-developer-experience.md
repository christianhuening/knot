# Developer Experience Implementation Plan (Plan 11)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make knot installable, runnable, and contributable from a cold clone in under 5 minutes. Today: there is no `README`, no `LICENSE`, no quickstart; the Makefile still has `spike.*` targets; backend edits require a manual `cargo run` restart; environment variables are only documented inside `knot-config/src/lib.rs`. After Plan 11: clone → `make dev` → working app at `http://localhost:5173`.

**Architecture:**
- **License:** Apache-2.0 (decided). Adds `LICENSE` at root + the standard header is NOT mandated on every file (Apache-2.0 only requires the NOTICE/LICENSE in the distribution).
- **README:** single root file. What knot is, what it isn't, screenshot/asciicast pointer, quickstart, architecture link, contributing link, license. ~150 lines.
- **ARCHITECTURE.md:** one page. Crates table, request lifecycle diagram (ASCII), CRDT data flow, where to find the spec for deeper reading. Links to the existing `docs/superpowers/specs/2026-06-01-knot-foundation-design.md` for the long form.
- **CONTRIBUTING.md:** how to set up, the test layout, plan-driven workflow expectation, the dev-compose constraint (no testcontainers), how to add a migration.
- **`.env.example`** documents every `KNOT_*` env var with safe local defaults.
- **`make dev`** boots dev-compose Postgres + cargo-watch backend + Vite frontend in a single foreground process group. Ctrl+C tears everything down. Cargo-watch reruns the server on Rust edits (~3 s cycle); Vite reruns the SPA on TS edits (~150 ms cycle).
- **Makefile rename:** `spike.server` → `dev.server`, `spike.web` → `dev.web`. The "spike" naming is leftover from Plan 1 and is now misleading.
- **`make migrate.create <name>`** scaffolds a new timestamped migration file.

**Tech Stack:** `cargo-watch` (installed lazily by the make target), GNU make process substitution, `trap` for clean shutdown. No new Rust or TS deps.

**Predecessors:** Plans 1-10 + Plan 7 (everything that makes knot a usable product). HEAD as of plan-write time.

**Out of scope** (intentionally deferred):
- **Devcontainer / Codespaces config** — useful for browser-based contributors but adds maintenance. Plan 12 territory.
- **`pre-commit` hooks** — `cargo fmt --check` + `eslint --max-warnings 0` would be nice but is a separate ergonomic choice (some contributors hate them).
- **Asciicast / screenshot in README** — placeholder text only for now; recording one is a one-off task.
- **Translated READMEs** — single-language for v0.1.
- **Roadmap doc** — the `docs/superpowers/plans/` directory IS the roadmap. Don't duplicate.

---

## File map

```
knot/
├── README.md                                   (new) front-door doc
├── LICENSE                                     (new) Apache-2.0 full text
├── NOTICE                                      (new) attribution per Apache-2.0
├── CONTRIBUTING.md                             (new) setup + workflow expectations
├── ARCHITECTURE.md                             (new) one-page system overview
├── .env.example                                (new) documented KNOT_* defaults
├── Makefile                                    (modify) rename spike.* → dev.*, add dev, migrate.create
└── docs/
    └── superpowers/
        └── README.md                           (new) index of plans + outcome docs
```

---

## Conventions

- **README tone:** Active voice. Direct. The reader is a senior engineer deciding whether to deploy or contribute. No marketing prose.
- **Make targets:** every new target gets a `## description` comment so `make help` lists it.
- **`make dev`:** prints a header showing the URLs (`backend :3000, frontend :5173`) then `wait`s on the child processes; `trap` propagates SIGINT.
- **`make migrate.create`:** uses `date +%Y%m%d%H%M%S` for the timestamp; mirrors the convention in `migrations/20260602000001_v0_1_schema.sql`.
- **Don't break existing CI.** The Makefile keeps `test`, `lint`, `e2e`, `compose.up` etc. intact — only renames and adds.

---

## Task overview

| # | Title | LOC ≈ |
|---|---|---|
| 1 | LICENSE + NOTICE | 200 (text) |
| 2 | README.md | 150 |
| 3 | ARCHITECTURE.md | 120 |
| 4 | CONTRIBUTING.md | 100 |
| 5 | `.env.example` | 50 |
| 6 | Makefile: rename + dev.server with cargo-watch | 40 |
| 7 | Makefile: `make dev` orchestrator | 60 |
| 8 | Makefile: `make migrate.create NAME=foo` | 20 |
| 9 | docs/superpowers/README.md (plan index) | 60 |
| 10 | Outcome doc | 0 |

---

## Task 1: LICENSE + NOTICE

**Files:**
- Create: `/home/nik/Development/knot/LICENSE`
- Create: `/home/nik/Development/knot/NOTICE`

- [ ] **Step 1: LICENSE**

Fetch the Apache-2.0 full text from the SPDX repository or paste the canonical text from [apache.org/licenses/LICENSE-2.0.txt](https://www.apache.org/licenses/LICENSE-2.0.txt). It's a fixed 11 KB file — paste verbatim, no edits.

- [ ] **Step 2: NOTICE**

Create `/home/nik/Development/knot/NOTICE`:

```
knot
Copyright 2026 Niklas Voss

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

This product bundles several dependencies (see Cargo.toml +
web/package.json). Each retains its own license; see those files for
attribution.
```

- [ ] **Step 3: Commit**

```bash
git add LICENSE NOTICE
git commit -m "license: Apache-2.0"
```

---

## Task 2: README.md

**Files:**
- Create: `/home/nik/Development/knot/README.md`

- [ ] **Step 1: Write**

```markdown
# knot

A self-hosted, collaborative knowledge base. Like Notion or Confluence — but
your data lives on your hardware, the source is yours to read, and the
real-time editor is built on CRDTs (no central operational-transform server
to wedge).

- **Backend:** Rust (axum + yrs + sqlx + tokio + mimalloc), static musl
  binary, ~20 MB scratch image.
- **Frontend:** React 18 + Tiptap + TanStack Query + Zustand.
- **Storage:** PostgreSQL 16 (single database, no Redis).
- **Auth:** local credentials + OIDC (tested against Dex).
- **Deploy:** Helm chart at `deploy/helm/knot/`. Multi-arch image.

## Status

v0.1 — feature-complete for single-workspace teams. Production-ready enough
to dogfood; not yet hardened (no rate limits on auth, no PrometheusRule
templates, no image push CI). See `docs/superpowers/plans/` for the
roadmap.

## Quickstart

```bash
git clone https://github.com/voss/knot
cd knot
make compose.up                            # boot Postgres + Dex
cp .env.example .env                       # KNOT_* defaults
make dev                                   # backend + frontend with live reload
```

Open `http://localhost:5173`. The first visit lands on `/setup` — create
the workspace owner.

### Requirements

- Rust 1.83+ (workspace `rust-toolchain.toml` pins this)
- Node 20+
- pnpm 9+ (`corepack enable pnpm` works)
- Docker (for the dev-compose Postgres + Dex)

The Nix flake at `flake.nix` pins all of the above; `direnv allow` is the
zero-friction path.

## Run the tests

```bash
make test                # cargo + vitest
make e2e                 # Playwright (needs compose.up)
make lint                # clippy + fmt --check + tsc + eslint
```

## Architecture

See `ARCHITECTURE.md` for the one-page overview. The long-form design spec
is at `docs/superpowers/specs/2026-06-01-knot-foundation-design.md`. Every
plan landed since (Plans 3-11) has an outcome doc at
`docs/superpowers/research/`.

## Deploy

```bash
helm install knot ./deploy/helm/knot \
  --set database.url='postgres://...' \
  --set session.key="$(openssl rand -base64 32)"
```

See `deploy/helm/knot/README.md` for the full install guide,
external-secret pattern, and OIDC setup.

## Observability

`/api/healthz` (liveness), `/api/readyz` (readiness), `/metrics` on
port 9090 (Prometheus). Import `deploy/grafana/knot.json` into Grafana 9+.
SLOs: `docs/SLO.md`.

## Contributing

`CONTRIBUTING.md` covers the setup, the test infrastructure (no
testcontainers — reuse the dev-compose Postgres), the plan-driven
workflow, and how to add a migration.

## License

Apache-2.0. See `LICENSE`.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: front-door README"
```

---

## Task 3: ARCHITECTURE.md

**Files:**
- Create: `/home/nik/Development/knot/ARCHITECTURE.md`

- [ ] **Step 1: Write**

Cover (one page, no chapter dividers):

1. **System diagram** (ASCII):

   ```
   Browser (SPA)
     │ HTTPS + WSS (single origin in prod)
     ▼
   knot-server (axum, Rust)
   ├─ /auth/*    → knot-auth      (Argon2id, OIDC, sid+csrf cookies)
   ├─ /api/*     → knot-storage   (sqlx, Postgres)
   ├─ /collab/:id WS  → knot-crdt (yrs Room actor)
   └─ /metrics    → knot-obs      (metrics-exporter-prometheus)
                            │
                            ▼
                       PostgreSQL
                       ├─ users, workspaces, sessions
                       ├─ documents, document_grants
                       ├─ doc_updates, doc_snapshots
                       └─ acl_invalidations (LISTEN/NOTIFY)
   ```

2. **Crates table:**

   | Crate | Role |
   |---|---|
   | `knot-server` | axum router, middleware, route handlers |
   | `knot-auth` | password hashing, session creation, OIDC client |
   | `knot-config` | figment-based `KNOT_*` env loader |
   | `knot-storage` | sqlx stores (users, workspaces, docs, grants, snapshots, updates) |
   | `knot-crdt` | yrs Engine, Room actor, PgBus over LISTEN/NOTIFY |
   | `knot-docs` | ACL evaluation + listener |
   | `knot-markdown` | markdown ↔ ProseMirror canonical schema |
   | `knot-obs` | logging, OTLP traces, Prometheus exporter |
   | `knot-test-support` | `fresh_db()` against dev-compose |
   | `tools/schemagen` | generates `schema.rs` + `schema.ts` from `tools/schema.json` |

3. **CRDT data flow:**
   - Client edits → Y.Doc.update → KnotProvider WS → server
   - Server: Room actor → yrs apply → persist update → optional snapshot → LISTEN/NOTIFY peers
   - Peers (other replicas) replay updates from the bus into their in-memory Room
   - On WS connect: SyncStep1 (client's state vector) → server sends SyncStep2 (missing updates) → real-time SyncUpdate frames

4. **Why we made the choices we did:**
   - **Yjs/yrs over Automerge** — JS interop is free, and the protocol is small.
   - **Postgres LISTEN/NOTIFY over Redis** — one fewer datastore.
   - **mimalloc** — small image footprint, ARM-friendly.
   - **Static musl + scratch** — operationally simplest container.

5. **Pointers:**
   - Long-form spec: `docs/superpowers/specs/2026-06-01-knot-foundation-design.md`
   - Per-plan rationale: `docs/superpowers/research/`
   - SLOs: `docs/SLO.md`

- [ ] **Step 2: Commit**

```bash
git add ARCHITECTURE.md
git commit -m "docs: ARCHITECTURE — one-page system overview"
```

---

## Task 4: CONTRIBUTING.md

**Files:**
- Create: `/home/nik/Development/knot/CONTRIBUTING.md`

- [ ] **Step 1: Write**

Cover:

1. **Setup** — same quickstart as the README, plus `make lint`, `make fmt`.
2. **Plan-driven workflow** — non-trivial work lands as a plan in
   `docs/superpowers/plans/`. Each plan has an outcome doc in
   `docs/superpowers/research/` after merge. Bug fixes and one-line tweaks
   don't need plans.
3. **Tests** — Rust uses `cargo nextest`; web uses Vitest + Playwright.
   Integration tests use `knot_test_support::fresh_db()` — **never
   testcontainers** (see the project memory file for why; thousands of
   leaked containers OOM'd the host twice).
4. **Migrations** — create with `make migrate.create NAME=add_foo_column`;
   never edit a landed migration (write a forward-only follow-up).
5. **Schema changes** — if you touch the editor schema, regenerate
   server + client artefacts with `make schema.gen` and commit both.
6. **Commit format** — Conventional Commits (`feat:`, `fix:`, `test:`,
   `docs:`, `chore:`, `build:`, `ci:`).
7. **PR expectations** — `make lint && make test && make e2e` green
   before review. Each plan ships with its own e2e if it touches the
   user-visible surface.
8. **License grant** — by submitting a PR you license the contribution
   under Apache-2.0.

- [ ] **Step 2: Commit**

```bash
git add CONTRIBUTING.md
git commit -m "docs: CONTRIBUTING guide"
```

---

## Task 5: `.env.example`

**Files:**
- Create: `/home/nik/Development/knot/.env.example`
- Modify: `/home/nik/Development/knot/.gitignore` — ensure `.env` is ignored (it likely already is)

- [ ] **Step 1: Write**

```bash
# Sample env file for local development. Copy to .env and tweak.
# All KNOT_* names match `crates/knot-config/src/lib.rs::Config`.

# --- Required ---
KNOT_DATABASE_URL=postgres://knot:knot@localhost:5432/knot
KNOT_SESSION_KEY=local-dev-key-32-bytes-aaaaaaaaaaa

# --- Server addr + base URL ---
KNOT_ADDR=:3000
KNOT_BASE_URL=http://localhost:5173

# --- SPA fallback (set by Dockerfile in production; local dev uses Vite) ---
# KNOT_WEB_DIST=/web/dist

# --- Logging + metrics ---
KNOT_LOG_LEVEL=info
KNOT_LOG_FORMAT=pretty
KNOT_METRICS_ADDR=:9090

# --- CRDT tuning ---
KNOT_SNAPSHOT_EVERY_N=200
KNOT_SNAPSHOT_IDLE_SEC=30
KNOT_ROOM_IDLE_EVICT_SEC=300

# --- Optional: OIDC against the dev Dex ---
# KNOT_OIDC_ENABLED=true
# KNOT_OIDC_ISSUER=http://localhost:5556/dex
# KNOT_OIDC_CLIENT_ID=knot
# KNOT_OIDC_CLIENT_SECRET=knot-dev-secret
# KNOT_OIDC_REDIRECT_URL=http://localhost:5173/auth/oidc/callback
# KNOT_OIDC_AUTO_PROVISION=always

# --- Optional: OTLP traces ---
# KNOT_TRACING_ENABLED=true
# KNOT_OTLP_ENDPOINT=http://localhost:4317
```

- [ ] **Step 2: Verify .gitignore**

```bash
grep -E "^\.env$" .gitignore || echo ".env" >> .gitignore
```

- [ ] **Step 3: Commit**

```bash
git add .env.example .gitignore
git commit -m "docs: .env.example with documented KNOT_* defaults"
```

---

## Task 6: Makefile — rename + cargo-watch

**Files:**
- Modify: `/home/nik/Development/knot/Makefile`

- [ ] **Step 1: Rename `spike.*` targets**

Find the two lines:

```make
spike.server: ## run the spike WebSocket server on :3000
spike.web: ## run the spike SPA via Vite on :5173 (proxies /collab to :3000)
```

Rename to:

```make
dev.server: ## run knot-server with cargo-watch (auto-restart on edit)
	@command -v cargo-watch >/dev/null 2>&1 || cargo install cargo-watch
	cargo watch -q -x "run --bin knot-server"

dev.web: ## run the SPA via Vite on :5173 (proxies /api,/auth,/collab to :3000)
	cd web && $(PNPM) dev
```

Update the `.PHONY` declarations accordingly. Keep `spike.server` / `spike.web` as DEPRECATED aliases that print a one-line notice and run the new target — gives anyone with a `spike.*` muscle memory a soft landing:

```make
.PHONY: spike.server spike.web
spike.server: ## (deprecated) alias for dev.server
	@echo "spike.server is deprecated; use 'make dev.server'"
	@$(MAKE) dev.server
spike.web: ## (deprecated) alias for dev.web
	@echo "spike.web is deprecated; use 'make dev.web'"
	@$(MAKE) dev.web
```

- [ ] **Step 2: Verify**

```bash
make help | grep -E "dev\.|spike\."
```

Should list both old + new with the deprecation note.

- [ ] **Step 3: Commit**

```bash
git add Makefile
git commit -m "build: rename spike.* → dev.* + cargo-watch on dev.server"
```

---

## Task 7: `make dev` via concurrently

Uses `concurrently` (npm pkg) for clean colored prefixed output (`[server]`, `[web]`) and proper SIGINT propagation, rather than raw make `&` + `trap` which interleaves unlabeled output.

**Files:**
- Modify: `/home/nik/Development/knot/web/package.json` — add `concurrently` dev dep + `dev:all` script
- Modify: `/home/nik/Development/knot/Makefile` — add `make dev` target

- [ ] **Step 1: Add concurrently**

```bash
cd /home/nik/Development/knot/web
pnpm add -D concurrently
```

- [ ] **Step 2: Add `dev:all` script to `web/package.json`**

Edit the `"scripts"` block. Add:

```json
"dev:all": "concurrently -k -n server,web -c blue,magenta \"cd .. && cargo watch -q -x 'run --bin knot-server'\" \"vite\""
```

Flags:
- `-k` — kill all on first exit (so dying server kills Vite too)
- `-n server,web` — prefix labels
- `-c blue,magenta` — color per process

- [ ] **Step 3: Add the make target**

```make
.PHONY: dev
dev: compose.up ## boot Postgres + backend (cargo-watch) + frontend (Vite) with live reload
	@command -v cargo-watch >/dev/null 2>&1 || cargo install cargo-watch
	@echo ""
	@echo "  knot dev"
	@echo "  ────────"
	@echo "  backend     http://localhost:3000"
	@echo "  frontend    http://localhost:5173"
	@echo "  metrics     http://localhost:9090/metrics"
	@echo ""
	@echo "  Ctrl+C to stop both."
	@echo ""
	cd web && $(PNPM) dev:all
```

- [ ] **Step 4: Smoke test**

```bash
# Manual (not part of automated verification):
make dev
# Watch for "[server] listening on 0.0.0.0:3000" and "[web] ready in N ms"
# Edit crates/knot-server/src/main.rs — [server] cargo-watch rebuilds
# Edit web/src/App.tsx — [web] Vite HMRs
# Ctrl+C — concurrently kills both cleanly
```

For automated verification, just `make -n dev` (dry-run) to confirm syntax, and confirm `web/package.json` `dev:all` script exists.

- [ ] **Step 5: Commit**

```bash
git add web/package.json web/pnpm-lock.yaml Makefile
git commit -m "build: 'make dev' via concurrently — prefixed live-reload"
```

---

## Task 8: Makefile — `make migrate.create`

**Files:**
- Modify: `/home/nik/Development/knot/Makefile`

- [ ] **Step 1: Add the target**

```make
.PHONY: migrate.create
migrate.create: ## scaffold migrations/<ts>_<NAME>.sql; usage: make migrate.create NAME=add_foo
	@if [ -z "$(NAME)" ]; then \
	  echo "usage: make migrate.create NAME=<short_snake_name>" >&2; \
	  exit 2; \
	fi
	@TS=$$(date -u +%Y%m%d%H%M%S); \
	  FILE="migrations/$${TS}_$(NAME).sql"; \
	  printf -- "-- %s\n-- Created %s\n\n" "$(NAME)" "$$(date -u +%Y-%m-%d)" > "$$FILE"; \
	  echo "created $$FILE"
```

- [ ] **Step 2: Smoke test**

```bash
make migrate.create NAME=test_target
ls migrations/*test_target* && rm migrations/*test_target*
```

Expected: file created with the timestamp prefix; manual cleanup.

- [ ] **Step 3: Commit**

```bash
git add Makefile
git commit -m "build: make migrate.create NAME=<name> scaffolder"
```

---

## Task 9: docs/superpowers/README.md — plan index

**Files:**
- Create: `/home/nik/Development/knot/docs/superpowers/README.md`

- [ ] **Step 1: Write**

```markdown
# Plans + outcomes

This directory is knot's planning log. Every non-trivial change lands as a
**plan** (an upfront task-by-task implementation document) and, on merge,
gets an **outcome doc** capturing what landed, what was non-obvious, and
what's still deferred.

## Plans landed

| # | Date | Topic | Plan | Outcome |
|---|---|---|---|---|
| 3 | 2026-06-01 | Auth (local + OIDC discovery) | (n/a) | (n/a) |
| 4 | 2026-06-01 | Documents + ACL | (n/a) | (n/a) |
| 5 | 2026-06-02 | CRDT Room Actor + Persistence | (n/a) | [research/2026-06-02-plan5-outcome.md](research/2026-06-02-plan5-outcome.md) |
| 6 | 2026-06-02 | Frontend Shell | [plans/2026-06-02-frontend-shell.md](plans/2026-06-02-frontend-shell.md) | [research/2026-06-02-plan6-outcome.md](research/2026-06-02-plan6-outcome.md) |
| 8 | 2026-06-02 | Auth Completion (change password, invite-with-password, OIDC e2e) | [plans/2026-06-02-auth-completion.md](plans/2026-06-02-auth-completion.md) | [research/2026-06-03-plan8-outcome.md](research/2026-06-03-plan8-outcome.md) |
| 9 | 2026-06-03 | Deployment (Helm + multi-arch image) | [plans/2026-06-03-deployment.md](plans/2026-06-03-deployment.md) | [research/2026-06-03-plan9-outcome.md](research/2026-06-03-plan9-outcome.md) |
| 10 | 2026-06-03 | Observability | [plans/2026-06-03-observability.md](plans/2026-06-03-observability.md) | [research/2026-06-03-plan10-outcome.md](research/2026-06-03-plan10-outcome.md) |
| 7 | 2026-06-03 | UI Polish | [plans/2026-06-03-ui-polish.md](plans/2026-06-03-ui-polish.md) | [research/2026-06-03-plan7-outcome.md](research/2026-06-03-plan7-outcome.md) |
| 11 | 2026-06-03 | Developer Experience | [plans/2026-06-03-developer-experience.md](plans/2026-06-03-developer-experience.md) | (this plan) |

## On deck

See `docs/superpowers/research/` outcome docs' "Carryforward" sections for what each plan owner recommended next. Common candidates:

- **Plan 12 — Production hardening** (rate-limit auth, NetworkPolicy, image push on tag, PrometheusRule, WS reconnect e2e)
- **Plan 13 — File uploads / images** (Notion-style image embeds)
- **Plan 14 — Full-text search**

## How to add a plan

1. Brainstorm via superpowers:brainstorming (or solo) to scope.
2. Write the plan via superpowers:writing-plans → `plans/<date>-<topic>.md`.
3. Execute via superpowers:subagent-driven-development.
4. On merge, write `research/<date>-<topic>-outcome.md` capturing status, gates, what's deferred, carryforward.
5. Add a row to the table above.
```

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/README.md
git commit -m "docs: superpowers plan index"
```

---

## Task 10: Outcome doc

**Files:**
- Create: `/home/nik/Development/knot/docs/superpowers/research/2026-06-03-plan11-outcome.md`

- [ ] **Step 1: Write**

Same shape as the Plan 9/10/7 outcome docs:

- **Status** (GO / etc.)
- **Gates** — `make help`, `make -n dev` (dry-run), file existence checks for README/LICENSE/etc.
- **What landed** — table of commits + tasks.
- **What's deferred** — devcontainer, asciicast, pre-commit, roadmap doc.
- **Carryforward** — recommend Plan 12 (production hardening) next.

- [ ] **Step 2: Commit**

```bash
git add docs/
git commit -m "docs: Plan 11 outcome"
```

---

## Self-review checklist

- [ ] `cargo test --workspace` green (unchanged — no Rust changes besides cargo-watch which is dev-only)
- [ ] `pnpm tsc/lint/test/playwright` green (unchanged — no web changes)
- [ ] `make help` lists all new targets with `## descriptions`
- [ ] `make -n dev` dry-runs without errors
- [ ] `make migrate.create NAME=foo` produces a file with the expected name
- [ ] `make migrate.create` (no NAME) prints usage and exits 2
- [ ] `README.md`, `LICENSE`, `NOTICE`, `CONTRIBUTING.md`, `ARCHITECTURE.md`, `.env.example`, `docs/superpowers/README.md` all exist
- [ ] `.env` is in `.gitignore`
- [ ] No file claims a license different from Apache-2.0
- [ ] Manual: `make dev` boots all three processes and Ctrl+C tears them down cleanly
- [ ] Manual: edit a Rust file → server rebuilds; edit a TS file → Vite HMRs
