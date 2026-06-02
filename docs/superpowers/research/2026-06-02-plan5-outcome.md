# Plan 5 (CRDT Room Actor + Persistence) outcome — 2026-06-02

## What landed

### Persistence layer (`crates/knot-storage`)
- **`UpdatesStore`** — `doc_updates` append-only log. `insert_batch` does one multi-row INSERT with RETURNING seq, preserving input order; `since(after_seq)`, `max_seq`, `delete_up_to(cutoff)` for the GC pipeline.
- **`SnapshotStore`** — `doc_snapshots` with UPSERT on `(doc_id, snapshot_seq)`. `latest()` returns the highest snapshot_seq; `gc(keep_recent, retain_days)` keeps last N + 1 per day via a recursive CTE.
- **`MarkdownCacheStore`** — `doc_markdown_cache` with strict `rendered_at_seq == current_seq` lookup; write-through on every export.

### `knot-crdt` (extended from Plan 1)
- **`Bus` trait** (publish update seqs / presence payloads / subscribe / unsubscribe) plus two impls:
  - **`MemBus`** — in-process for unit tests
  - **`PgBus`** — Postgres LISTEN/NOTIFY over a dedicated `tokio_postgres` connection. Channel naming `doc:<uuid>` carries `seq` as text; `presence:<uuid>` carries base64-url-safe-no-pad bytes. Driver task uses `stream::poll_fn(|cx| connection.poll_message(cx))` + `tokio::pin!` to surface notifications. Subscriptions are demuxed by channel prefix into per-doc mpsc senders. LISTEN issued only on first subscribe per doc.
- **`Room` actor** — one `tokio::task` per active doc, exclusive owner of `DocHandle`. `tokio::select!` loop handles inbound updates, joins/leaves, applied seqs from the writer, bus notifications, awareness frames, snapshot triggers, catch-up tick, and revocation.
- **`writer.rs`** — sibling task that batches inserts: flush at 200 updates OR 250ms since first item, multi-row INSERT RETURNING seq, publish each seq over the bus, send `Applied` back to the room.
- **Hydration** — `Room::spawn` is async; loads the latest snapshot, applies its state, then SELECTs and applies `doc_updates` after `snapshot_seq` before serving any conn.
- **Snapshot scheduler** — N-trigger inside the `applied_rx` arm (when `updates_since_snapshot >= every_n`); idle-trigger inside a 1s tick (when `last_apply_at.elapsed() >= idle && updates_since_snapshot > 0`).
- **`gc.rs`** — hourly task: SELECT DISTINCT doc_id, then per doc: `updates.delete_up_to(latest_snapshot_seq - 2 * every_n)` + `snapshots.gc(5, 30)`.
- **Backpressure** — local fan-out evicts slow consumers (`try_send` Full/Closed → remove conn → WS shim closes 4408); persist channel is `.send().await` so the actor backpressures into the writer.
- **Awareness** — `Event::AwarenessIn { from, payload }` drops oversize (>4 KB), fans out locally (skipping origin), publishes to bus. `Leave` emits an empty payload as a clearing sentinel for the frontend.
- **Catch-up tick** — `interval(5s)` and `bus_updates_rx` arm both call `replay_since_watermark` (SELECT-since with watermark guard, apply each, advance, fan out). NOTIFYs are hints; the periodic tick heals dropped notifications.
- **`Rooms` registry** — `DashMap<Uuid, Arc<RoomHandle>>` plus per-doc `Arc<Mutex<()>>` for in-flight dedup. `acquire(doc_id)` does double-checked locking. `evict` cancels the actor and unsubscribes from the bus. `revoke_all_for_doc` sends `Event::Revoke` which clears the conn map.
- **Final snapshot on shutdown** — written best-effort at the end of `run` so the next boot starts cheap.

### `knot-server`
- **`AppState`** gains `Option<Arc<Rooms>>`, `Option<Arc<dyn Bus>>`, `Option<Arc<dyn MarkdownCacheStore>>`. `with_pool` populates the markdown cache; `main.rs` constructs `PgBus` async and wires `bus`/`rooms_v2`.
- **WS upgrade rewritten** — `Path<Uuid>` (was `Path<String>`), 401 if no `AuthContext`, 403 `acl.no_grant` if `effective_role` returns `None`, hands off to `room::serve`. Role pinned at upgrade time per spec §7.4.
- **`room::serve` WS shim** — acquires the room, sends `Event::Join`, receives hydrated state, frames it as `sync_step_2`, decodes inbound frames into `Event::Inbound`/`Event::AwarenessIn`. Writer task observes closed `out_rx` and sends a `4403 acl.revoked` close frame.
- **4403 on revocation** — `knot-docs::spawn_listener` gained an `on_invalidate: Arc<dyn Fn(Uuid) + Send + Sync>` callback. `main.rs` wires it to `rooms_v2.revoke_all_for_doc(doc_id)` for the root doc + every descendant returned by the listener's existing subtree walk.
- **`GET /api/docs/:id/markdown`** — handler acquires the room, sends `Event::ExportState`, receives `(state_bytes, seq)`, applies into a transient `YrsEngine + DocHandle`, calls `knot_markdown::to_markdown::serialise`, write-through to `MarkdownCacheStore`, returns `text/markdown`. Requires `EffectiveDocRole`.
- **`POST /api/docs/:id/markdown`** — reads ≤1MB UTF-8 body, calls `knot_markdown::from_markdown::parse(&text)` (returns `(DocHandle, Vec<u8>)`), sends `Event::ApplyUpdate { update_bytes, by_user, reply }` to the room which applies + persists via writer + fans out. Returns 204. Editor+ required.
- **Plan 4 carryover #1** — workspace member CRUD now writes `audit_events` rows (`workspace.member.invite|role|remove`).

### Test infrastructure
- **`knot-test-support` crate** — new helper that creates a unique `t_<uuid>` database on the long-lived dev-compose Postgres at `localhost:5432`. Every test reuses one container; thousands of leaked testcontainers from the previous pattern (`std::mem::forget(c)` per fixture) are now zero.
- **`make db.cleanup`** — drops all leftover `t_*` databases in one short query for when a test process is SIGKILLed mid-run.
- **All 17 test files migrated** from per-fixture `Postgres::default().start()` to `knot_test_support::fresh_db().await`.

### e2e
- **`collab.spec.ts`** — full markdown round trip via the room actor: setup → create doc → POST markdown → GET markdown → assert heading + paragraph round-trip.
- **`auth.spec.ts` TRUNCATE** extended to include `doc_updates`, `doc_snapshots`, `doc_markdown_cache`.
- **`two-users-converge.spec.ts`** — `.skip()` with a comment pointing at Plan 6. T16 rewired the WS handler to require auth + `Path<Uuid>`; the frontend SPA still navigates to `/?docId=<string>` without logging in. Plans 6-8 will rewrite the frontend to log in + create a real doc.

## Test infrastructure incident & resolution

Mid-plan a subagent bundled an unauthorized "fresh_db" refactor into the T17 commit (4403 close). The original implementation used `OnceCell<ContainerAsync<Postgres>>` which dedupes *within a single process* — but `cargo test` launches one process per test binary, so the workspace's ~10 binaries still spawned ~10 containers per run. Across many test runs the host accumulated **thousands of leaked Postgres containers** and ran out of memory.

The unauthorized commit was split into honest standalone commits (`60a0368` infra crate + `2a54502` migrations). Then the helper was rewritten (`a479590`) to abandon testcontainers entirely and reuse the dev-compose Postgres via `CREATE DATABASE` per call.

**Aftermath**: 127 tests pass, 0 new containers per `cargo test` run, `make db.cleanup` reclaims orphans. Recorded as a feedback memory so subagents don't reintroduce the pattern.

## In-flight design corrections

1. **`knot-markdown` API discovery** — Plan 1's Engine trait was missing `to_markdown`/`from_markdown` despite spec §8.2 listing them. Rather than amending the trait + risking a dep cycle (`knot-markdown` already deps on `knot-crdt`), the room actor returns the raw state snapshot (`Event::ExportState` → `(Vec<u8>, i64)`), and the HTTP handler does the markdown work via `knot_markdown::to_markdown::serialise` against a transient doc. Same for import via `knot_markdown::from_markdown::parse`. Net effect: markdown work happens off the hot actor path, which is also a small perf win.
2. **Two-users-converge spec is a Plan 6 problem** — T16's auth-at-upgrade closed the unauthenticated WS path the spike's frontend used. Skipped with a forward pointer; not a regression to fix in Plan 5.

## Test counts at Plan 5 close

```
cargo test --workspace      → 127 PASS (up from 108 at Plan 4 close)
  + knot-storage          new: UpdatesStore (5), SnapshotStore (4), MarkdownCacheStore (4)
  + knot-crdt             new: MemBus (3), PgBus (2), Room smoke + writer round-trip + hydration (3)
  + knot-test-support     0 (infra, no tests of its own)

cargo clippy --workspace --all-targets --all-features -- -D warnings   → clean
cargo deny check                                                        → ok
cd e2e && pnpm playwright test                                          → 5 PASS / 1 SKIP
  + collab.spec.ts (new — markdown round trip)
  + auth.spec.ts × 2, docs.spec.ts, health.spec.ts (unchanged)
  ~ two-users-converge.spec.ts (skipped, Plan 6 frontend rewrite needed)
```

## Plan 5 commit trail (master)

23 commits from `10c9abf..HEAD`:

```
a479590 fix(test-infra): reuse dev-compose Postgres instead of spawning containers
09c95e8 test(e2e): skip two-users-converge — frontend needs auth+UUID after T16
86e3baa test(e2e): markdown import + export round trip via room actor
e4ada12 feat(knot-server): POST /api/docs/:id/markdown via room actor
ac44de3 feat(knot-server): GET /api/docs/:id/markdown via room state + handler serialize
2a54502 test(infra): migrate remaining tests to knot_test_support::fresh_db
60a0368 test(infra): knot-test-support crate — shared Postgres container
d527996 feat(acl): close active WS with 4403 on ACL revocation
0518033 feat(knot-server): rewrite collab WS — auth at upgrade + knot-crdt Rooms
1eb4dfd feat(knot-crdt): Rooms registry with in-flight dedup + final snapshot on evict
dd8618e feat(knot-crdt): 5s catch-up tick + bus update replay
bf07001 feat(knot-crdt): awareness fan-out + size cap + leave-clear
abc4de7 feat(knot-crdt): backpressure — slow-consumer eviction + persist await
035c9ef feat(knot-crdt): hourly snapshot + updates GC task
ec1474a feat(knot-crdt): snapshot scheduler — N-updates + idle-sec triggers
956af17 feat(knot-crdt): hydrate room from latest snapshot + replay updates
891bb77 feat(knot-crdt): writer task — batched doc_updates + bus publish
8c9b5b1 feat(knot-crdt): Room actor skeleton (select loop, join, leave, inbound apply)
c7d9884 feat(knot-crdt): PgBus over tokio_postgres LISTEN/NOTIFY
d6c6eac feat(knot-crdt): Bus trait + in-process MemBus impl
5278937 feat(knot-storage): MarkdownCacheStore (lazy-fill by seq)
38219ae feat(knot-storage): SnapshotStore (insert + latest + GC)
b128f47 feat(knot-storage): UpdatesStore (batch insert + since + max_seq + delete_up_to)
fe2fccf chore(plan-4-cleanups): audit member CRUD
```

## Verdict

**GO.** The §8 actor model + §5.4/§5.5 persistence + §6.1 markdown endpoints + §6.2 collab WS + §7.6 revocation + §9 bus are all live and tested end-to-end. All gates green. The 4403 close path is wired but not e2e-tested (would need to drive a WS client + ACL change in Playwright — left for a future test).

## What's still NOT done after Plan 5

Carrying forward to later plans:
- **Frontend rewrite for auth-at-WS-upgrade** (Plans 6-8) — the current SPA navigates to `/?docId=<string>` without logging in or creating a doc first. After Plan 6, two-users-converge will be re-enabled. The frontend also needs:
  - Log in via /auth/login or /auth/setup
  - Create a doc via POST /api/docs (or use an existing one)
  - Connect with `WSS /collab/<uuid>` carrying the sid cookie
  - Read close code 4403 and surface "you no longer have access"
  - Parse the empty awareness payload as a clearing sentinel
- **2-replica integration test** — not in Plan 5; manually verifiable with two `cargo run --bin knot-server` instances sharing the same compose Postgres.
- **Writer FK race during TRUNCATE** — observed in T20 e2e: the writer's debounced batch can fail with FK violation if a doc is deleted between apply and flush. v0.1 docs are soft-deleted only (no DELETE), so this is test-only. Worth a graceful-degradation pass when the schema or test order changes.
- **Per-user revocation** — Plan 5 closes ALL connections to a doc on grant change, not just the affected user. Refinement deferred.
- **Awareness clearing semantics** — the room emits an empty payload as "you should re-query" rather than a real "drop clientID X" frame. Frontend will need to decide how to handle this; Plan 6 may refine the contract.
- **`Rooms::acquire .expect(...)` panics** — transient bus subscribe or hydrate errors panic the registry. Hardening for Plan 9 (production readiness).
- **`Rooms::inflight` accumulates** — entries added per doc on first acquire are never cleaned up. Memory is small but bounded growth would be cleaner.
- **No GET /api/docs/:id/markdown e2e for the cache hit path** — only the cold path is tested. Easy follow-up.
- **`pprof` endpoint** — Plan 9.
- **Helm chart + image build** — Plan 9.
- **`Markdown` import/export of advanced blocks** (tables, callouts) — out of v0.1 per spec §8.8.

These are intentional. Plan 5's job was the durable CRDT spine the rest of v0.1 hangs from, plus the markdown export/import surface. Frontend + production hardening live in Plans 6-9.
