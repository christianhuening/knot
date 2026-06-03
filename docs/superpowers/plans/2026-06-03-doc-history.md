# Doc History & Restore Implementation Plan (Plan 20)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (recommended) or superpowers:executing-plans.

**Goal:** A snapshot list per doc with preview + restore. Click a snapshot, see its markdown. Restore replaces the doc's current content with that snapshot's markdown.

**Architecture:**
- The CRDT writer task already persists snapshots to `doc_snapshots` per Plan 5. Each row has `(doc_id, snapshot_seq, state_bytes, created_at)`. No new schema.
- **List endpoint:** `GET /api/docs/:doc_id/history` returns metadata (id, seq, created_at, byte_size) for all snapshots of a doc, newest first. Editor+ ACL.
- **Preview endpoint:** `GET /api/docs/:doc_id/history/:seq/markdown` loads the snapshot bytes into a transient `YrsEngine` doc and serializes via `knot_markdown::to_markdown::serialise`. Returns `text/markdown`. Editor+ ACL.
- **Restore endpoint:** `POST /api/docs/:doc_id/history/:seq/restore` is the interesting one. It re-renders the snapshot to markdown, then constructs a y-update that **replaces** the live room's content with the markdown-parsed state. Forward-only (Yjs is monotonic). The Plan 5 markdown POST cold-import path already does "blank doc → markdown content"; we generalize it to "any doc → replace with markdown content" by computing a delete-everything + apply-update pair in a single transaction. Editor+ ACL.
- **Honest about what restore is:** the round-trip is lossy in the same way Plan 5's markdown export is — anything not in the canonical schema doesn't survive. Documented in the UI ("Restore re-imports the snapshot as markdown — formatting outside the canonical block set is not preserved.").

**Predecessor:** Plan 17 (HEAD `3a057f5`).

**Out of scope:**
- **Diff view between snapshots.** Defer (user explicit).
- **Per-paragraph blame.** Defer.
- **Visual timeline scrubber.** Defer.
- **Snapshot pinning** (so the writer task doesn't GC them). Not needed for v0.1 — the snapshot retention policy is "all of them" today.
- **Rewriting/erasing history.** Restore creates a forward update; the snapshot you restored from stays in the list.
- **Restore to anonymous viewer.** Owner/Editor only; public share links remain read-only.

---

## File map

```
crates/knot-server/
├── src/routes/api/history.rs                              (new) GET list, GET preview, POST restore
├── src/routes/api/mod.rs                                  (modify) +history router
└── tests/history_integration.rs                            (new) list + preview + restore + ACL

crates/knot-crdt/
└── src/room.rs                                            (modify) +Event::ReplaceWithMarkdown(markdown, tx)
                                                                  performs a transaction that deletes
                                                                  current XmlFragment content + applies
                                                                  the parsed-markdown update.

crates/knot-storage/
└── (no changes — SnapshotStore::latest_n + by_seq already in place from Plan 5;
   verify before writing routes)

web/
├── src/lib/history.api.ts                                 (new) historyApi
├── src/features/docs/HistoryDrawer.tsx                    (new) side panel listing snapshots
├── src/features/docs/DocPage.tsx                          (modify) "History" link in header
└── src/routes.tsx                                         (no change — drawer mounts inside DocPage)

e2e/flows/
└── history.spec.ts                                        (new) edit twice → restore → old text returns
```

---

## Conventions

- **Snapshot listing limit:** server caps at 50 most recent snapshots per request. UI shows the same; pagination is a follow-up.
- **`created_by` is not in `doc_snapshots` today** (Plan 5 stored snapshots without an author column). Either accept that the UI shows "system" / no author, OR add a migration to record `created_by` going forward. Choosing accept-and-document for v0.1; future plan can add the column + backfill nulls.
- **Restore is irreversible-from-here**, but a new snapshot of the pre-restore state likely exists. The UI warns: "Anything you've typed since the latest snapshot will be replaced."
- **`SnapshotStore` API:** verify the signature before writing routes. Expected methods: `latest(doc_id)`, `list(doc_id, limit)`, `by_seq(doc_id, seq)`. If `list`/`by_seq` don't exist, add them in Task 2.

---

## Tasks

| # | Title | LOC ≈ |
|---|---|---|
| 1 | knot-storage: SnapshotStore list + by_seq | 80 |
| 2 | knot-crdt: Room Event::ReplaceWithMarkdown | 160 |
| 3 | knot-server: GET /api/docs/:id/history | 100 |
| 4 | knot-server: GET .../history/:seq/markdown (preview) | 100 |
| 5 | knot-server: POST .../history/:seq/restore | 140 |
| 6 | Server integration tests (6 cases) | 220 |
| 7 | web: historyApi | 60 |
| 8 | web: HistoryDrawer + DocPage link | 220 |
| 9 | e2e: edit → restore → old text | 130 |
| 10 | Outcome doc | 0 |

---

## Task 1: SnapshotStore — list + by_seq

Read `crates/knot-storage/src/snapshot_store.rs`. The trait probably has `latest(doc_id)` and `put(doc_id, seq, bytes)`. Add (if missing):

```rust
async fn list(&self, doc_id: Uuid, limit: i64) -> Result<Vec<SnapshotMeta>, _>;
async fn by_seq(&self, doc_id: Uuid, seq: i64) -> Result<Option<DocSnapshot>, _>;
```

Where `SnapshotMeta` is a light struct `{ snapshot_seq, byte_size, created_at }`.

Verify + commit.

---

## Task 2: Room Event::ReplaceWithMarkdown

In `crates/knot-crdt/src/room.rs`:

1. Add an enum variant:
   ```rust
   ReplaceWithMarkdown {
       markdown: String,
       reply: tokio::sync::oneshot::Sender<Result<u64, String>>,  // returns the new seq
   }
   ```

2. In the actor's `select!` arm, handle the new event:
   - Parse markdown → DocHandle (via `knot_markdown::parse`).
   - Encode the parsed doc's state as an update.
   - In a yrs transaction on the live `DocHandle`: delete the XmlFragment's children, then apply the encoded update.
   - Persist via the existing writer task and bump seq.
   - Reply with `Ok(new_seq)`.

> **Note:** Yrs transactions can compose multiple ops. The cleanest pattern is `apply_update` after a manual "clear" inside one `transact_mut`. The `knot_markdown::parse` output should give you a DocHandle whose state encodes the markdown as a fresh y-doc; encoding that as an update yields the bytes to apply.

3. Use `engine.apply_update(&doc, &bytes)` after the clear. If the engine doesn't have a clear primitive, do the deletion via `yrs` directly — the editor schema's top-level node is a single XmlFragment named `prosemirror`.

Verify by writing a small unit test inside `room.rs` that creates a room, applies some updates, replaces, and confirms the new content matches the markdown round-trip.

Commit.

---

## Task 3: GET /api/docs/:doc_id/history

Create `crates/knot-server/src/routes/api/history.rs`. The list handler:
- Editor+ ACL via `acl.effective_role`.
- Calls `state.snapshots.list(doc_id, 50)`.
- Returns `Json([{ snapshot_seq, byte_size, created_at }, ...])`.

Mount in `routes/api/mod.rs`.

Verify + commit.

---

## Task 4: GET .../history/:seq/markdown

Same file, second handler:
- Editor+ ACL.
- Calls `state.snapshots.by_seq(doc_id, seq)` → `Option<DocSnapshot>`.
- Builds a transient `YrsEngine` doc, applies the snapshot bytes, serializes via `knot_markdown::to_markdown::serialise`.
- Returns `text/markdown; charset=utf-8`.

Verify + commit.

---

## Task 5: POST .../history/:seq/restore

Same file, third handler:
- Editor+ ACL.
- Loads snapshot bytes, builds the markdown (same as Task 4).
- Sends the markdown to the room via `Event::ReplaceWithMarkdown { markdown, reply }`.
- Returns 204 on success, 500 on failure.

Verify + commit.

---

## Task 6: Integration tests

`crates/knot-server/tests/history_integration.rs`:

1. **Empty doc → list returns at least 1 snapshot** (initial creation triggers a snapshot — verify or seed via `state.snapshots.put` if needed).
2. **List respects limit + ordering** (newest first).
3. **Preview returns markdown matching what was exported at that seq.**
4. **Restore replaces content.** Edit doc twice (writes via room.tx), snapshot after each, restore to the older one, verify current content matches the older markdown.
5. **Viewer cannot list/preview/restore** (403).
6. **Anon (no sid) → 401.**

Use the existing `state_with_seeded_user` + `make_doc` pattern from `blobs_integration.rs`. Seeding history entries via `state.snapshots.put(...)` directly is fine — bypassing the writer task makes tests deterministic.

Verify + commit.

---

## Task 7: historyApi

`web/src/lib/history.api.ts`:

```ts
export type SnapshotMeta = {
  snapshot_seq: number;
  byte_size: number;
  created_at: string;
};

export const historyApi = {
  list(docId: string)         → ApiResult<SnapshotMeta[]>
  preview(docId, seq)         → ApiResult<string>  // raw markdown text
  restore(docId, seq)         → ApiResult<void>
};
```

For `preview`, since the server returns `text/markdown` (not JSON), implement with raw `fetch` + `text()` like the public-doc client does.

Verify + commit.

---

## Task 8: HistoryDrawer

`web/src/features/docs/HistoryDrawer.tsx`:

A right-side drawer (mirror the dialog pattern from PermissionsDialog but on the right). Sections:
- Header: "History"
- List of snapshots (timestamp + size). Click selects one.
- Right pane: markdown preview (rendered as plain `<pre>` so users see what they'd restore).
- Footer: "Restore this snapshot" button (with a confirm prompt).

State: TanStack Query keys `["history", docId]` for the list, `["history", docId, seq]` for the active preview.

Mount inside `DocPage` (similar to how `<Outlet />` mounts PermissionsDialog as a child route — but for v0.1 simpler to use Zustand UI state: `historyOpen: { docId } | null`, a button in the DocPage header toggles it).

Modify `DocPage.tsx` to add a "History" link next to "Permissions".

Verify + commit.

---

## Task 9: e2e

`e2e/flows/history.spec.ts`:

1. Owner sets up + creates doc + types "First version" + waits for snapshot (or POST `/api/docs/:id/markdown` to force cache fill, then directly call `state.snapshots.put` via SQL? — Simpler: skip the snapshot-triggering and assert the API surface manually).
2. Actually — for a clean e2e, seed snapshots directly via SQL `INSERT INTO doc_snapshots (...)` in the test setup, or use `await page.evaluate(fetch /api/docs/:id/history)` to verify the list is non-empty after some editing time.
3. Open history drawer, click oldest snapshot, see preview, click Restore.
4. Editor content becomes the snapshot's text.

This is the trickiest e2e in the plan. The snapshot policy is "every 200 updates OR 30s idle". To trigger one quickly, override `KNOT_SNAPSHOT_EVERY_N=1` for this test (set in `playwright.config.ts`'s webServer env — but that affects all tests). Alternative: typing 200 chars to trigger. Alternative: directly insert a snapshot via SQL.

Direct-SQL seeding is the most deterministic. Pre-create a snapshot with known content before the test, then assert restore brings that content back.

Verify + commit.

---

## Task 10: Outcome doc

Status, gates, what landed, what was non-obvious, what's deferred (diff view, blame, scrubber). Add Plan 20 row.

---

## Self-review

- [ ] `cargo test --workspace` green (+6 history cases)
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `pnpm tsc/lint/test` clean
- [ ] `pnpm playwright test` 25/25 (was 24)
- [ ] Manual: type → wait 30s → snapshot appears in History → click → see markdown → Restore → editor reflects the snapshot
- [ ] Manual: viewer opens History → either it's hidden in the UI OR all actions 403 (server enforces — UI hides for clean UX)
