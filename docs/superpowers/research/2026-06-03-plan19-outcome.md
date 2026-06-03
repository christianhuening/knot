# Plan 19 Outcome — Comments & Mentions

**Status:** GO_WITH_CONCERNS. All 18 tasks landed; **26/26 e2e pass, 0 skipped**. One real gap noted: server-side mention bridge from `pg_notify` to the WS room actors isn't wired (the frontend handler is). Tracked as Plan 19.5.

**Verdict:** Comments are functional end-to-end: inline thread anchors via Yjs `RelativePosition`, replies, six-emoji reactions, resolve/unresolve, @mention parsing, soft-delete. The biggest UX surface remaining for v0.1 is real-time mention delivery — a focused half-day follow-up.

## What landed

| Commit | Task | Subject |
|---|---|---|
| ce62c6c | T1 | migrations: comments + comment_reactions tables |
| e6f301e | T2 | knot-storage: CommentStore + PgCommentStore + Reaction types |
| 3b1957f | T3 | knot-server: POST/GET comments + replies (with reactions inlined) |
| 4d524cd | T4 | knot-server: resolve/unresolve + reactions endpoints |
| e33192b | T5 | knot-server: edit (author) + delete (author or workspace owner) |
| 3acc91c | T6 | knot-server: publish @mention notifications on the bus |
| e6aa597 | T7 | test(knot-server): comments — CRUD + ACL + resolve + reactions + mention |
| b4a2208 | T8 | web: commentsApi |
| 05a3024 | T9 | web: anchor.ts — Y.RelativePosition encode/decode |
| 7855e6b | T10-14 | web: CommentSidebar + CommentThread + CommentComposer + reactions + resolve |
| bb756dd | T15 | web: selection Add-comment float + CommentSidebar mount in DocPage |
| e35c135 | T16 | web: MSG_MENTION handler in KnotProvider + mention toast in KnotEditor |
| 989cd8b | T17 | test(e2e): comments flow — thread, reply, reaction, resolve, show-resolved |
| e7d1a41 | hot | fix: clamp Add-comment float position; e2e CSRF + cache-bust |

T18 is this outcome doc.

## Gates

- `cargo test --workspace` — **195 passed** (+11 comments cases), 2 skipped, 0 failed
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean
- `pnpm tsc/lint/test` — clean (+5 anchor tests, 24 total)
- `pnpm playwright test` — **26 passed, 0 skipped**

## Architecture summary

**Storage:** two Postgres tables. `comments` is 1-level threaded — `parent_id IS NULL` for thread roots, `parent_id = thread_id` for replies. `position_y BYTEA` holds a Yjs `encodeRelativePosition` output (base64-decoded server-side); null means whole-doc. `anchor_text` is a snippet at creation time, used as a fallback label when the position can't be resolved. `comment_reactions` is `(comment_id, user_id, emoji)` with a composite PK enforcing toggle semantics.

**Endpoints under `/api/docs/:doc_id/comments`** — all guarded by the existing `require_doc_role_mw` layer:
- POST + reply (Editor+) | GET (Viewer+, `?include_resolved=true|false`)
- PATCH (author only) | DELETE (author or workspace Owner; soft delete)
- resolve / unresolve (Editor+, thread root only)
- reactions add / remove (Editor+, emoji allow-list of 6)

**ACL escalation:** workspace Owner can delete any comment (Notion behavior). The handler reads `AuthContext.role`.

**@mention extraction:** server-side regex `(?:^|\s)@(\w+)` against `display_name`, case-insensitive. Mentioned user_ids included in the comment response. Also: server publishes `pg_notify('comment_mentions', json)` after create/reply/edit.

**Frontend:**
- `commentsApi` — thin wrapper, mirrors `shares.api.ts` / `grants.api.ts` style.
- `anchor.ts` — `encodeAnchor` / `decodeAnchor` using `ySyncPluginKey.getState(editor.state)?.binding?.mapping`. 5 vitest cases.
- `CommentSidebar` — right-side drawer, groups by `thread_id`, sort active first, "Show resolved" toggle.
- `CommentThread` + `CommentComposer` — root + replies, member lookup via `["members"]` query, reply composer per thread.
- `MentionPicker` — `@<query>` detection in textarea + arrow-key dropdown of members. `useMentionPicker` hook composes via `{ textareaProps, picker }`.
- Reactions row — six-emoji mini-picker; click an emoji to toggle.
- Resolve button — Editor+ only; sidebar hides resolved by default.
- Floating "Add comment" button — appears on non-empty editor selection (Editor+). Encodes the selection's start as `position_y`, opens the sidebar with a `pendingAnchor` for the new-thread composer.
- `KnotProvider` learned message type 4 (`MSG_MENTION`) with a JSON payload. `ProviderEvents.mention` dispatches; `KnotEditor` subscribes and toasts when the current user is in `user_ids`.

## What was non-obvious

**Floating button positioning intercepted toolbar clicks.** The first selection-update placed the float at `coords.top - editorDom.top - 32`. For selections near the top of the editor, that's negative — and since the editor host has `position: relative`, the absolute child renders outside its parent's box, lands over the toolbar's hit-area, and intercepts `toolbar-bold` / `toolbar-h1` / etc. Two existing e2e specs broke. Fixed by flipping the button BELOW the selection when there's no room above. Caught by `editor-toolbar.spec.ts` + `command-palette.spec.ts` failing on regression-run.

**TanStack Query staleTime hid the e2e's directly-posted comment.** The test bypasses the in-app composer (which doesn't have a "new thread without anchor" UI surface) and posts directly via `fetch`. The sidebar's `["comments", docId, false]` query is fresh for 30s, so the toggle-on-then-off dance returned cached "no comments" data. Fix: `page.reload()` after the POST. Real users hit this via the composer's TanStack mutation which invalidates correctly — the e2e workaround is honest about the path it's taking.

**CSRF token must be carried on direct fetches.** The Plan 6 CSRF middleware enforces `X-CSRF-Token` on unsafe methods. The first e2e draft posted JSON without it and the server quietly 403'd; the test consumed `r.json()` and got an empty object, so `threadRes.id` was empty and `expect(threadId).toBeTruthy()` failed. Lesson: when an e2e does `page.evaluate(fetch ...)`, always read the cookie and set the header.

**rust-analyzer's `missing field` warnings are nearly always stale.** Three plans in a row have surfaced "missing field" diagnostics that `cargo check` immediately disproves. The pattern is consistent: AppState gets a new field added in a subagent commit, IDE doesn't refresh, but the build is green. Adding a brief `cargo check` after every subagent return is worth the 0.5 seconds.

**Migration test catches the `share_tokens` / `comments` style regressions cheaply** — but only if you update `migrations_apply.rs` proactively, BEFORE applying the migration. Plans 17 and 20 both hit "test fails because expected list doesn't include the new table"; Plan 19 fixed this by editing the test in the same commit as the migration.

## What's still deferred — most notable

**T16 server-side bridge is missing.** `crates/knot-server/src/routes/api/comments.rs::broadcast_mentions` calls `pg_notify('comment_mentions', json)` after each mention-bearing write. But:

- `knot_crdt::Bus` has `publish` (for CRDT updates) and `publish_presence` (for awareness). There's no `publish_mention` channel.
- `PgBus` doesn't `LISTEN` on `comment_mentions`.
- `Room::run` has no `select!` arm for mention messages.
- The `collab_upgrade` WS handler doesn't fan out MSG_MENTION frames.

So the frontend's `KnotProvider.mention` listener will never fire. Mentions are extracted server-side and persisted via the comment body itself; only the toast is missing. This is honest "comment shipped, push delivery deferred". Tracked as **Plan 19.5 — Mention push bridge** (~5 tasks: extend Bus, listen in PgBus, route through Room::run, MSG_MENTION encode in WS shim, e2e with two contexts).

## Other deferrals

- Multi-level reply threading.
- Persistent notifications table + bell UI (Plan 21).
- Email notifications on mention (Plan 18 + Plan 21).
- Markdown formatting in comment bodies.
- Comment edit history.
- Reactions beyond the fixed six.
- Activity feed.

## Carryforward

Recommended next:
1. **Plan 19.5 — Mention push bridge.** Closes the T16 gap; 4-5 tasks.
2. **Plan 18 — Email / SMTP.** Foundation for invite-with-password emails + password reset + mention emails.
3. **Plan 21 — Persistent notifications + bell UI.** Builds on Plan 19.5 and Plan 18.

## Files of interest

| Path | Role |
|---|---|
| `migrations/20260603171347_comments.sql` | tables + indexes |
| `crates/knot-storage/src/comments.rs` | CommentStore + Pg impl |
| `crates/knot-server/src/routes/api/comments.rs` | all endpoints + mention regex + pg_notify |
| `crates/knot-server/tests/comments_integration.rs` | 11 cases |
| `web/src/lib/comments.api.ts` | client wrapper |
| `web/src/features/comments/anchor.ts` | Y.RelativePosition helpers + 5 unit tests |
| `web/src/features/comments/CommentSidebar.tsx` | drawer with grouping + toggle |
| `web/src/features/comments/CommentThread.tsx` | root + replies + reactions + resolve |
| `web/src/features/comments/CommentComposer.tsx` | textarea + submit + mention picker hook |
| `web/src/features/comments/MentionPicker.tsx` | @ autocomplete dropdown |
| `web/src/features/editor/KnotEditor.tsx` | floating "Add comment" + mention toast |
| `web/src/features/editor/KnotProvider.ts` | MSG_MENTION handler (waiting on server bridge) |
| `e2e/flows/comments.spec.ts` | thread / reply / react / resolve round-trip |
