# Plan 11 Outcome — Developer Experience

**Status:** GO. All 10 tasks landed; all gates green.

**Verdict:** A new contributor (or a returning maintainer who hasn't touched the repo in a month) can now go from `git clone` to a working app in three commands. The front-door docs answer the "what is this and why should I care" question, the Makefile names match the actual product (no more `spike.*`), and `make dev` boots the full live-reload stack in one foreground process. Recommended next: **Plan 12 (production hardening)**.

## What landed

Plan 11 commits (HEAD `86fb6a9`):

| Commit | Task | Subject |
|---|---|---|
| ced3e9e | T1  | license: Apache-2.0 |
| 435b8b6 | T2  | docs: front-door README |
| 79f6969 | T3  | docs: ARCHITECTURE — one-page system overview |
| 4c35fbb | T4  | docs: CONTRIBUTING guide |
| 566fb16 | T5  | docs: .env.example with documented KNOT_* defaults |
| f6d5888 | T6  | build: rename spike.* → dev.* + cargo-watch on dev.server |
| d56a7bc | T7  | build: 'make dev' via concurrently |
| 63cb90c | T8  | build: make migrate.create NAME=<name> scaffolder |
| 86fb6a9 | T9  | docs: superpowers plan index |

T10 is this outcome doc.

## Gates

- `make help` lists every new target with its `## description`.
- `make migrate.create` (no NAME) prints usage + exits 2.
- `make migrate.create NAME=foo` produces `migrations/<timestamp>_foo.sql` with a header.
- `cargo test --workspace` unchanged — clean (no Rust code changes).
- `pnpm tsc`/`lint`/`test` unchanged — clean (one new dev dep: `concurrently`).
- `pnpm playwright test` unchanged — 19/19 green.
- All required docs at repo root: `README.md`, `LICENSE`, `NOTICE`, `CONTRIBUTING.md`, `ARCHITECTURE.md`, `.env.example`. `.env` is in `.gitignore`.

## Architecture summary

**`make dev` orchestrator:** `concurrently` is a small (~50 KB) dev dep that runs multiple commands with labeled, color-prefixed output. The new `web/package.json` script `dev:all` invokes:

```
concurrently -k -n server,web -c blue,magenta
  "cd .. && cargo watch -q -x 'run --bin knot-server'"
  "vite"
```

`-k` kills all on first exit (so a dying server tears down Vite too). `-n` adds the `[server]` / `[web]` prefix. `-c` colors them. The make target `dev` depends on `compose.up` so Postgres is healthy before the two reloaders start, then runs `cd web && pnpm dev:all`.

**Backend live reload:** `cargo-watch -q -x "run --bin knot-server"` rebuilds + relaunches on any source change. Cold rebuild is ~30 s; subsequent incremental builds are ~3 s. The make target lazily installs `cargo-watch` if missing (`command -v` check + `cargo install`).

**Frontend live reload:** Vite HMR — already worked, just needed to be wired into the same orchestrator.

**Migrations:** `make migrate.create NAME=<name>` uses `date -u +%Y%m%d%H%M%S` for the timestamp prefix, matching the convention in the existing `migrations/20260602000001_v0_1_schema.sql`. Forward-only — never edit a landed migration.

## What was non-obvious

**The repo had `spike.*` targets long after the spike was rewritten.** Plan 6 replaced the spike SPA entirely but the make targets kept the old name. Renamed to `dev.*` and added deprecated aliases that print a one-line warning and run the new target — gives muscle memory a soft landing.

**`.env` was already in `.gitignore` but not surfaced anywhere.** Newcomers had no way to discover what `KNOT_*` vars existed without reading `crates/knot-config/src/lib.rs`. The new `.env.example` surfaces the full list with sensible local defaults and commented-out OIDC + OTLP blocks.

**`concurrently` won over `make + trap`.** The plan originally had `make dev` use raw `&` backgrounding + `trap 'kill 0' EXIT INT TERM`. That works but the interleaved unlabeled output is ugly. The user suggested `concurrently` mid-plan — adopted before execution. Cost: one dev dep. Benefit: colored, labeled, properly-propagating Ctrl+C.

**Apache-2.0 text doesn't need fetching.** A pristine copy already lived in `~/.cargo/registry/.../LICENSE-APACHE`. Copied verbatim; no SPDX lookup needed.

## What's still deferred

- **Devcontainer / Codespaces config** — useful for browser-based contributors but adds maintenance. Defer until someone actually asks.
- **`pre-commit` hooks** — `cargo fmt --check` + `eslint --max-warnings 0` would be nice but some contributors hate them. Out of scope.
- **Asciicast / screenshot in README** — placeholder text only. A `vhs` recording of the setup flow would be the right way to do this; one-off task whenever someone has 30 minutes.
- **Translated READMEs** — single-language for v0.1.
- **Roadmap doc** — `docs/superpowers/README.md` (T9) IS the roadmap. Don't duplicate.

## Carryforward for the next plan

**Plan 12 — Production hardening** is the recommendation, in roughly this order:

1. Rate limit `/auth/login` and `/auth/password` per-user (lockout after N failures within a window).
2. NetworkPolicy templates in the chart (default-deny, allow ingress on http + metrics).
3. PrometheusRule template (rendered when `serviceMonitor.enabled=true`). SLO doc already lists the burn-rate signals — translate them to PromQL.
4. Image push CI on tag (`docker buildx build --push --platform linux/amd64,linux/arm64`).
5. WS reconnect e2e (`KnotProvider.scheduleReconnect` exists but is uncovered).

Other recommendations from prior outcome docs:
- **Plan 13** — File uploads / image embeds.
- **Plan 14** — Full-text search.

## Files of interest

| Path | Role |
|---|---|
| `README.md` | Front door — quickstart, deploy, contributing, license |
| `ARCHITECTURE.md` | One-page system overview + crate table + CRDT flow |
| `CONTRIBUTING.md` | Setup, plan workflow, test infra constraint, commit style |
| `.env.example` | Documented `KNOT_*` defaults |
| `LICENSE` + `NOTICE` | Apache-2.0 + attribution |
| `Makefile` | `make dev` (one command), `make migrate.create`, `dev.server`/`dev.web` |
| `web/package.json` | `dev:all` script driving `concurrently` |
| `docs/superpowers/README.md` | Plan + outcome index — the human-readable roadmap |
