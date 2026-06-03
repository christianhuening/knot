# Plan 20 Outcome — Doc History & Restore

**Status:** GO. All 10 tasks landed. **25/25 e2e pass.**

**Verdict:** Editors can now browse every snapshot of a doc, preview its markdown, and restore — content from any point in history comes back in one click. Restore is forward-only (Yjs-compatible): the snapshot's markdown is re-parsed into a single y-update that replaces the live content. Lossy on anything outside the canonical schema (same trade-off as Plan 5's markdown round-trip).

## What landed

Plan 20 commits (HEAD `595af04`):

| Commit | Task | Subject |
|---|---|---|
| 1133774 | T1 | knot-storage: SnapshotStore::list + by_seq + SnapshotMeta |
| fe1552d | T2 | knot-crdt: Room Event::ReplaceWithMarkdown (forward replace) |
| e04f26d | T3 | knot-server: GET /api/docs/:id/history (list) |
| 6d65cb1 | T4 | knot-server: GET /api/docs/:id/history/:seq/markdown |
| ae2bf7b | T5 | knot-server: POST /api/docs/:id/history/:seq/restore |
| 50a894f | T6 | test(knot-server): history list/preview/restore + ACL |
| 192ce0c | hot | test(knot-storage): migrations_apply — add share_tokens (Plan 17 regression fix) |
| 0803638 | T7 | web: historyApi (list/preview/restore) |
| 2ea5755 | T8 | web: HistoryDrawer + DocPage link (owner/editor) |
| 595af04 | T9 | test(e2e): history — type → snapshot → edit → restore round trip |

T10 is this outcome doc.

## Gates

- `cargo test --workspace` — **173 passed, 0 failed, 2 skipped** (+6 history integration cases; also fixed an unrelated regression in `migrations_apply` from Plan 17 that hardcoded the table list)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean
- `pnpm tsc/lint/test` — clean
- `pnpm playwright test` — **25 passed, 0 skipped**

## Architecture summary

**Snapshot listing & preview** are read-only — the `SnapshotStore` already persists every snapshot via the writer task, so list/by_seq are pure SQL. Preview reuses Plan 5's transient-doc pattern: load snapshot bytes into a fresh `YrsEngine` doc, serialize via `knot_markdown::to_markdown::serialise`.

**Restore is the interesting bit.** A new `Event::ReplaceWithMarkdown` on the room actor performs a single yrs transaction that:
1. Acquires the canonical XmlFragment ref **before** opening `transact_mut()` — avoiding a write-lock deadlock.
2. Clears all children via `remove_range`.
3. Applies the markdown-parsed update via `txn.apply_update(Update::decode_v1(...))`.

The room's existing writer task persists the result and fans it out to local connections. Result: the editor sees the restore as a normal CRDT update, no special client-side handling needed.

**ACL:** all three endpoints require Owner or Editor (not Viewer). Plumbed via the existing `require_doc_role_mw` layer on `docs::router()`, so the history routes live inside that subtree rather than as a standalone router. The handlers read `EffectiveDocRole` from request extensions — set by the middleware.

**UI:** a right-side `HistoryDrawer` component with two columns — list of snapshots (timestamp + size) on the left, markdown preview in a `<pre>` on the right, Restore button at the bottom. Owner/Editor see a `History` link in the DocPage header; Viewer doesn't. Restore uses `window.confirm` for safety.

**E2E approach:** the snapshot policy default is "every 200 updates OR 30s idle". For a deterministic e2e, set `KNOT_SNAPSHOT_EVERY_N=1` in the Playwright webServer env. Every editor update now triggers a snapshot; tests can observe history within a few hundred milliseconds.

## What was non-obvious

**Acquiring the XmlFragment before `transact_mut`.** First implementation called `doc.get_or_insert_xml_fragment("default")` inside the `transact_mut` scope, which acquires a write lock on the same doc that `get_or_insert_xml_fragment` tries to read. Deadlock. Moved the ref acquisition outside the transaction. Documented inline.

**The canonical fragment name is `"default"`, not `"prosemirror"`.** Checked `knot_markdown::from_markdown::parse` for the actual name. Two-line code review saved a debugging session.

**Migration apply test regression from Plan 17.** `migrations_apply_cleanly` hardcodes the expected table list. Plan 17 added `share_tokens` but didn't update the test — Plan 13 caught this for blobs but Plan 17 missed it. Fixed in `192ce0c`. Going forward, the test should be updated alongside every migration that adds a table — but the warning is buried in the test failure output, not in the migration scaffold. A future plan could add a `make migrate.create` post-hook that prints "remember to add the new table to migrations_apply.rs:expected".

**`knot-test-support` migration cache strikes again.** Third time in three plans. Adding a migration requires `cargo clean -p knot-test-support` before integration tests see the new table. Worth automating with a make target — or moving the test-support helper to use runtime migration discovery instead of `sqlx::migrate!`.

**Yrs API exploration via the existing `markdown.rs` and `room.rs` was faster than the docs.** The implementer mirrored the `Event::ApplyUpdate` arm to figure out the persistence trigger. Reading other tests beats reading the upstream API for figuring out method signatures and lifetimes.

## What's still deferred

- **Diff view between two snapshots.** User explicit deferral.
- **Per-paragraph blame.** Deferred.
- **Visual timeline scrubber.** Deferred.
- **Snapshot pinning** to prevent GC. The current `gc` policy is "keep all" so this is moot; revisit once retention becomes a knob.
- **`created_by` on snapshots.** The `doc_snapshots` table doesn't track an author. The UI shows "no author" implicitly. Future migration could add the column.
- **Restore preserves the new state in history.** Currently the post-restore state will itself get snapshotted on the next idle/N — visible in the drawer as the newest snapshot. That's the right behavior.
- **Notify the room's other connected clients about the restore via a toast.** Today they just see content change. Minor polish.
- **Better preview rendering than `<pre>`.** Plan 17's public-doc render path (pulldown_cmark inside an iframe) could be reused for the preview pane.
- **Restore from public share URL.** Out of scope — public is read-only by design.

## Carryforward

Last plan from the user's batch: **Plan 19 — Comments / inline mentions** (~15 tasks). Biggest remaining feature.

Other follow-ups noted during this plan:
- `make migrate.create` post-hook to remind about `migrations_apply.rs`
- Move `knot-test-support` to runtime migration discovery

## Files of interest

| Path | Role |
|---|---|
| `crates/knot-storage/src/snapshot_store.rs` | +list / +by_seq / SnapshotMeta |
| `crates/knot-crdt/src/room.rs` | Event::ReplaceWithMarkdown + unit test |
| `crates/knot-server/src/routes/api/history.rs` | three handlers under require_doc_role_mw |
| `crates/knot-server/tests/history_integration.rs` | 6 cases |
| `crates/knot-storage/tests/migrations_apply.rs` | added `share_tokens` (Plan 17 hotfix) |
| `web/src/lib/history.api.ts` | client wrapper (text/markdown for preview) |
| `web/src/features/docs/HistoryDrawer.tsx` | right-side drawer UI |
| `web/src/features/docs/DocPage.tsx` | History link, drawer mount |
| `e2e/flows/history.spec.ts` | round-trip e2e |
| `e2e/playwright.config.ts` | `KNOT_SNAPSHOT_EVERY_N=1` for deterministic snapshots |
