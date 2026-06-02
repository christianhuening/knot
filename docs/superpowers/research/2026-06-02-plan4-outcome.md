# Plan 4 (Documents & ACL) outcome — 2026-06-02

## What landed

- **Plan 3 carryovers cleared** (T1): `AppState.config: Arc<Config>` threaded from `main` through every handler; OIDC auto-provision policy now reads from `state.config` (not env at request time); the OIDC callback's existing-user "auto-add as Viewer" is now gated on `oidc_auto_provision != "off"`; unused `WorkspaceStoreError::NotFound` removed.
- **`crates/knot-storage`** grows real persistence:
  - `WorkspaceStore`: `list_members`, `update_role`, `remove_member`, `count_owners` + `Member` row type.
  - `DocStore` (replaces Plan 2 stub): `Document` + `list_alive` / `get` / `create` / `rename` / `move_to` / `archive` / `restore` / `siblings` / `descendant_ids`. Every mutation transactionally writes an `audit_events` row plus an `acl_invalidations` row (except `rename`, which doesn't change ACL).
  - `GrantStore`: `list` / `list_inherited` (recursive CTE, depth-ordered) / `put` (UPSERT) / `delete`. Both mutations write audit + invalidation rows.
  - `audit::record_in_tx` + `invalidations::record_in_tx` helpers that share the mutation's transaction.
  - `lexorank::between(a, b)` — pure-Rust base-36 sort-key generator with 50-iter monotonicity test.
- **`crates/knot-docs`** (NEW crate):
  - `acl::resolve(workspaces, grants, ws, doc, user) -> Option<WorkspaceRole>` taking the max of (workspace role, max matching grant role).
  - `AclCache` (moka 0.12, 60s TTL, 100k entries, `support_invalidation_closures()` ON so `evict_doc` actually works).
  - `spawn_listener(pool, cache)` task: `LISTEN acl_invalidate` → parse Uuid payload → evict the changed doc AND all its descendants → `run_pending_tasks` → GC the outbox row. Reconnects with 5s backoff on disconnect.
- **`crates/knot-server`** auth + routes:
  - `RequireDocRole` middleware reads `Path<DocIdParam>` (named struct, not positional Uuid — fixes a real bug for routes with 2 path params), resolves via `AclCache::effective_role`, inserts `EffectiveDocRole(role)` extension. 401 / 403 / 500 envelopes.
  - `routes::api::router(state)` mounts `csrf_mw` + `require_session_mw` over the `/api/*` subtree. `routes::api::workspace` exposes the 4 member endpoints with last-owner guard. `routes::api::docs` exposes list / create / get / patch / move / archive / restore. `routes::api::grants` is merged INTO the docs router so the 3 grant routes share the same `require_doc_role_mw` layer.
  - `main::run_server` spawns the ACL listener after `AppState` construction.
- **`/api/*` over HTTP**:
  - `GET /api/workspace`, `GET|POST /api/workspace/members`, `PATCH|DELETE /api/workspace/members/:id`
  - `GET|POST /api/docs`, `GET|PATCH|DELETE /api/docs/:id`, `POST /api/docs/:id/move`, `POST /api/docs/:id/restore`
  - `GET /api/docs/:id/grants`, `PUT|DELETE /api/docs/:id/grants/:principal`
- **e2e**: new `docs.spec.ts` exercises the full surface — setup → create root + child → get with effective_role → patch title → move to root → delete/restore → pre-provision bob via psql → invite bob → grant editor on root → list grants → delete grant. `auth.spec.ts` truncate extended to include the 4 new Plan 4 tables. `playwright.config.ts` set to `workers: 1` so the two specs don't race the `/auth/setup` slug.

## Workspace at end of Plan 4

```
knot/
├── tools/schemagen           Plan 1 — JSON → Rust+TS codegen
├── crates/knot-crdt          Plan 1 — Engine trait + yrs adapter
├── crates/knot-markdown      Plan 1 — MD round-trip
├── crates/knot-config        Plan 2 — figment + 8 OIDC fields
├── crates/knot-obs           Plan 2 — tracing/metrics/OTLP
├── crates/knot-storage       Plan 2 — sqlx pool + 4 stores       ★ extended with DocStore + GrantStore + audit + invalidations + lexorank + members CRUD
├── crates/knot-auth          Plan 3 — password/token/throttle/csrf/oidc
├── crates/knot-docs          Plan 4 — ACL resolver + cache + listener   ★ new
└── crates/knot-server        Plan 3+4 — auth + /api/* + RequireDocRole  ★ extended
```

## In-flight fixes during Plan 4 review

Five non-trivial corrections to the plan/code happened along the way; all are in the commit trail:

1. **T3 LexoRank `mid()` used base-36 ordinal space**, not raw ASCII. The spec's literal `mid()` would land outside `0-9a-z` (gap between `'9'` and `'a'`). Implementer's fix is what the spec's doc-comment actually demanded.
2. **T7 AclCache needed `support_invalidation_closures()`** on the builder. Without it, moka silently swallows `invalidate_entries_if` (the closures-disabled error). T7 alone would have shipped a no-op cache; T9's listener integration test caught it.
3. **T9 listener needed `run_pending_tasks()`** after each `evict_doc` to make eviction deterministic — moka's `invalidate_entries_if` is lazy. The cache crate gained a thin `pub async fn run_pending_tasks(&self)` to expose this.
4. **T10/T12 `require_doc_role_mw` switched from `Path<Uuid>` to `Path<DocIdParam>`**. axum's positional `Path<Uuid>` extractor errors with "Expected 1 but got 2 path arguments" when the route has a second param (`/api/docs/:id/grants/:principal`). Named-struct extraction is the canonical axum 0.7 fix; no caller in Plan 3 needed it.
5. **T13 plan code vs T16 test contract clashed** on `POST /api/docs/:id/move` with `{parent_id: null}`. Plan code did `body.parent_id.or(cur.parent_id)` (null → keep current). T16's test asserted null → root. Settled on `body.parent_id` (null and missing both move to root) — the only sane v0.1 behavior without an `Option<Option<Uuid>>` serde dance.

## Post-T16 follow-up fixes

Two refinements landed after the final cross-cutting review, per user direction:

1. **`fix(knot-docs): listener walks subtree and evicts descendants on inherit change`** (`d0ecae4`) — Spec §7.5 says "per-replica listeners walk the affected subtree and evict cache entries." The original T9 listener only evicted the changed `doc_id`. Now it also calls a new `DocStore::descendant_ids(doc_id)` (recursive CTE on `parent_id`) and evicts each descendant. Closes the 60-second-TTL window where a child's cached effective_role could be stale after an ancestor's grant changed.
2. **`fix(knot-server): move with unknown after_id/before_id falls to end of siblings`** (`e5fbcd7`) — `create` and `move` had asymmetric anchor-fallback behavior. With a stale `after_id` from the client, `create` placed the new doc at the end of siblings; `move` silently re-ordered it to the first slot (`sort_key="m"`). Aligned `move`'s three fallback paths to "end of siblings" via a shared `end_of_siblings` closure. Regression test asserts `moved.sort_key > C.sort_key` when `after_id` is a freshly minted (non-existent) Uuid.

## Test counts at Plan 4 close

```
cargo test --workspace         → 108 PASS (up from ≈80 at Plan 3 close)
  + knot-docs           6 (4 acl + 2 listener integration)
  + knot-storage       32 (added: 2 workspace_members, 5 documents, 4 grants,
                           1 invalidations, 1 descendant_ids — total +13)
  + knot-server        22 (added: docs_integration with 2 cases)
  (rest unchanged)

cd e2e && pnpm playwright test → 5/5 PASS
  + docs.spec.ts (new — workspace + docs + grants flow)

cargo deny check               → advisories + bans + licenses + sources ok
cargo clippy --workspace -D warnings → clean
```

## Plan 4 commit trail (master)

19 commits between `67935d2..HEAD`:

```
e5fbcd7 fix(knot-server): move with unknown after_id/before_id falls to end of siblings
d0ecae4 fix(knot-docs): listener walks subtree and evicts descendants on inherit change
5b19ba9 test(e2e): workspace + docs + grants happy-path flow
3014dd8 fix(knot-server): move null parent + grants path arity
fae24b3 feat(knot-server): grants endpoints under shared RequireDocRole layer
f98ac54 feat(knot-server): DELETE /api/docs/:id + restore + integration test
f0fa222 feat(knot-server): PATCH /api/docs/:id + POST /api/docs/:id/move
5f2fd0b feat(knot-server): GET/POST /api/docs + GET /api/docs/:id with effective_role
3a746bf feat(knot-server): /api/* gets CSRF + RequireSession; spawn ACL listener
c33fbb2 feat(knot-server): RequireDocRole middleware + AclCache wired into AppState
bd47754 feat(knot-docs): PgListener task evicts AclCache on acl_invalidate
2c820e8 test(knot-storage): mutations write to acl_invalidations outbox
8a76036 feat(knot-docs): AclCache (moka, 60s TTL) with evict_doc + evict_all
0b6faee feat(knot-docs): ACL resolver (workspace role + grant inheritance)
ae07986 feat(knot-storage): GrantStore with inheritance walk
54a7e8a feat(knot-storage): real DocStore with audit + invalidation outbox writes
1d26dd4 feat(knot-storage): LexoRank-style sort_key helper
0c04e63 feat(workspace): list/update/remove members + /api/workspace endpoints
b2e903b refactor: thread Config through AppState; gate OIDC existing-user; drop WorkspaceStoreError::NotFound
```

## Verdict

**GO.** The full §6.1 `/api/*` surface from the foundation spec is live and tested end-to-end. The cookie/CSRF/role middleware chain is wired correctly. The LISTEN/NOTIFY pipeline correctly evicts cache entries for the mutated doc *and its descendants*. All gates green.

## What's still NOT done after Plan 4

Carrying forward to later plans:
- **CRDT room actor + persistence** (Plan 5) — replaces the in-memory `Rooms` from the spike. Includes `doc_updates` / `doc_snapshots` hydration, snapshot/GC, and the `KNOT_SNAPSHOT_*` knobs.
- **`GET|POST /api/docs/:id/markdown`** (Plan 5) — needs the room actor for the live Y.Doc.
- **WebSocket close-frame 4403 on revocation** (Plan 5) — needs the room to broadcast to active sockets when ACL invalidation arrives.
- **Workspace member CRUD audit rows** — `audit_events` schema is in place; the `/api/workspace/members*` handlers currently don't write rows. Tracked for a small follow-up (≤1 LOC per endpoint via `audit::record_in_tx`).
- **`invalidations.rs` cosmetic** — `DELETE … WHERE doc_id = $1 AND created_at <= now()` has an always-true predicate. Functionally harmless, but worth simplifying to `WHERE doc_id = $1` when next touched.
- **Frontend UI for tree / members / grants** — Plans 6-8.
- **Helm + image build** — Plan 9.
- **Subtree eviction over very wide trees** — current CTE returns every descendant Uuid into Rust then iterates. For workspaces with >10k descendants under one root the listener could batch evictions or filter by `(doc_id, *)` directly. Not a v0.1 concern but worth a sanity-bench when content scales.
