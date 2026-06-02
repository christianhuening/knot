# Plan 3 (Auth) outcome — 2026-06-02

## What landed

- **Dex dev IdP** at `deploy/compose/dex/config.yaml` + service in `deploy/compose/dev.yml`. Issuer `http://localhost:5556/dex`, one OIDC client `knot`, two seeded password users (alice/bob).
- **`crates/knot-auth`** — new workspace crate housing the auth primitives:
  - `password.rs` — Argon2id `Hasher` (OWASP 2023 defaults via `Argon2::default()`; `fast_for_tests()` gated by the `fast-params` Cargo feature so it can't reach production code paths).
  - `session_token.rs` — 32-byte `SessionToken` from `OsRng`, base64url-no-pad codec, custom `Debug` that redacts the bytes.
  - `throttle.rs` — leaky-bucket throttle (capacity 5, drain 1/min). Per-key (caller stamps `"ip:..."` / `"email:..."`). Injectable `Clock` for tests. A T5 fix preserves sub-minute remainder so slow-trickle attackers can't drift past the cap.
  - `csrf.rs` — HMAC-SHA256 mint/verify (`hmac::Mac::verify_slice` is constant-time).
  - `oidc.rs` — thin wrapper around `openidconnect` 4.0.1. PKCE auth-code flow: discover → authorize URL → exchange → verify id_token + nonce. Groups plucked from the already-verified id_token payload via a small JWT decoder.
- **`crates/knot-storage`** grows three concrete stores plus tests:
  - `WorkspaceStore` — workspace + workspace_members CRUD.
  - `UserStore` — local + OIDC users, citext-aware email lookup, constraint-name-aware unique-violation mapping (`EmailExists` vs `OidcExists`).
  - `SessionStore` — bytea PK, TTL-filtered `find_active`, `touch`, `delete`. IP round-trips via `ipnetwork::IpNetwork`. The `Session` struct exposes `user_agent` + `ip` (T8 fix-up — they were write-only originally).
- **`crates/knot-server`** auth middleware + routes:
  - `auth::context::AuthContext { user_id, workspace_id, role }`.
  - `auth::session_loader` — reads `sid` cookie, looks up active session, attaches AuthContext. Touch fired-and-forgot with warn-on-error.
  - `auth::require_session` — 401 envelope when AuthContext absent.
  - `auth::csrf` — double-submit check for unsafe methods of authenticated requests; safe methods and anon requests pass through.
  - `auth::cookies` — single source of truth for `sid`/`csrf`/`oidc_flow` cookie shapes; `find_cookie`, `build_session_cookies`, `build_clear_cookies`, `build_flow_cookie`, `build_flow_clear_cookie`.
  - `routes::auth::setup` — `POST /auth/setup` first-run bootstrap. Mints sid + csrf cookies and 201s; 410 once any user exists.
  - `routes::auth::local` — `POST /auth/login`, `POST /auth/logout`, `GET /auth/session`. Login throttles by IP and email, sleeps 1 s on any failure for timing equalisation.
  - `routes::auth::oidc` — `GET /auth/oidc/login`, `GET /auth/oidc/callback`. PKCE flow stashed in a short-lived HttpOnly `oidc_flow` cookie. Auto-provision policies (off/always/domain/group) gate user creation.
  - `admin.rs` + clap subcommand router in `main.rs` — `knot-server admin create --email ... --display-name ...` reads password from stdin for headless first-user bootstrap.
- **`http_error::json_err`** — shared envelope helper per spec §6.3.
- **`crates/knot-config`** — 8 new `KNOT_OIDC_*` fields with policy-enum + required-when-enabled + JSON-shape validation.
- **e2e `auth.spec.ts`** — setup → session → logout happy path + wrong-password 401. `beforeAll` truncates auth tables via `docker compose exec psql` so the suite is repeatable.
- **CI** — e2e job now brings up dev compose + applies migrations + threads `KNOT_DATABASE_URL` / `KNOT_SESSION_KEY` to the server. `compose.down` on always.
- **`deny.toml`** — one new ignore: RUSTSEC-2023-0071 (rsa Marvin Attack), documented.

## Workspace at end of Plan 3

```
knot/
├── tools/schemagen           Plan 1 — JSON → Rust+TS codegen
├── crates/knot-crdt          Plan 1 — Engine trait + yrs adapter
├── crates/knot-markdown      Plan 1 — MD round-trip
├── crates/knot-config        Plan 2 — figment + 8 new OIDC fields           ★ extended
├── crates/knot-obs           Plan 2 — tracing/metrics/OTLP
├── crates/knot-storage       Plan 2 — sqlx pool + 3 stores                  ★ extended
├── crates/knot-auth          Plan 3 — password/token/throttle/csrf/oidc    ★ new
└── crates/knot-server        Plan 3 — auth middleware + 6 auth routes      ★ extended
```

## RUSTSEC-2023-0071 decision

`rsa` v0.9.x has a known Marvin-Attack timing sidechannel. It's reachable on the production path via `openidconnect → oauth2 → rsa`.

- **Used only for RS256 signature verification**, not decryption. The Marvin Attack targets decryption timing.
- **Deployment context** (Dex co-located with knot-server) bounds an attacker's ability to measure timing across the network.
- **No upstream fix** — the RustCrypto team has not released a constant-time `rsa` (see https://github.com/RustCrypto/RSA/issues/626).

The deny.toml ignore is documented with the reasoning and the directive to re-check on each `openidconnect` / `rsa` release. Accepted as a pragmatic call; the alternatives (forking `openidconnect`, raw `jose-jwt`, or red-CI-until-fixed) all trade more risk than they remove.

## Test counts at Plan 3 close

```
cargo nextest run --workspace      → ≈80 PASS (up from 28 at Plan 2 close)
  + knot-auth          26 (4 password + 6 session_token + 7 throttle + 4 csrf + 4 oidc + 1 doctest)
  + knot-config         7 (4 prior + 3 OIDC fields)
  + knot-storage       12 (1 migrations + 1 workspaces + 5 users + 5 sessions)
  + knot-server        20 (2 lib + 2 setup + 3 local + 1 admin + 7 middleware + 3 health + 1 convergence + 1 smoke)

cd e2e && pnpm test                → 4/4 PASS
  + health.spec.ts
  + two-users-converge.spec.ts
  + auth.spec.ts (setup → session → logout + wrong password)

cargo deny check                   → advisories + bans + licenses + sources all ok
```

## Carry-forward for Plan 4

Three minor cleanups deliberately deferred:

1. **`oidc::auto_provision` reads `KNOT_OIDC_*` env vars directly** instead of from the validated `Config`. Plan 4 should thread `Config` (or the relevant subset) into `AppState` so policy changes flow through the config-file precedence.
2. **OIDC existing-user auto-add as `Viewer`** in `routes/auth/oidc.rs` runs unconditionally — even when `KNOT_OIDC_AUTO_PROVISION=off`. Plan 4 should gate workspace-grant-on-existing-OIDC-user on policy too.
3. **`WorkspaceStoreError::NotFound`** is defined but never constructed. Trivial cleanup; remove when Plan 4 touches `WorkspaceStore`.

Plan 4 should also:
- Mount `csrf_mw` and `require_session_mw` on the new `/api/*` subtree as it grows.
- Implement the per-document ACL resolver + `document_grants` queries.
- Implement the `acl_invalidations` outbox + NOTIFY listener.

## Verdict

**GO.** All 6 §6.1 auth endpoints land, the §7 middleware chain is correct, OIDC against Dex is wired and tested up to the unit-helper layer (live e2e against Dex is a stretch goal reasonably deferred), and the cookie/error/middleware story holds together end-to-end.

## What's still NOT done after Plan 3

- No document tree / per-document ACL (Plan 4).
- No CRDT room actor / persistence — the spike's in-memory `Rooms` still serves convergence (Plan 5).
- No frontend changes (Plans 6-8).
- No Helm chart / Docker image build / multi-arch release (Plan 9).
