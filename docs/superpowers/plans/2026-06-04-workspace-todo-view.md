# Workspace Todo View Plan (Plan 31)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** A `/tasks` route lists all incomplete checklist items in the workspace, grouped by assignee. Click-through navigates to the source doc + scrolls to the task. Depends on Plan 30 having shipped checklists with `@mention` assignees.

**Architecture:**
- A new `doc_tasks` table indexed by `(workspace_id, assignee_user_id, completed_at)`.
- A `TaskIndexer` rebuilds rows for a given doc whenever its markdown cache refreshes (already happens server-side per doc snapshot — there's a hook).
- A minimal markdown walker extracts `- [ ]` / `- [x]` items, captures the leading mention (if any) as assignee, computes a stable task id from `(doc_id, item_index, line_hash)`.
- Indexing is idempotent: re-running on the same markdown gives the same rows.
- REST `GET /api/workspaces/me/tasks` returns the current user's open tasks (and recently completed, last 7d, optional).
- Frontend `/tasks` page: sidebar entry between "Documents" and "Members"; grouped by doc, with a "My tasks" filter and an "All assignees" filter for owners.

---

## Tasks

### T1: Migration — `doc_tasks`
```
CREATE TABLE doc_tasks (
  id              TEXT PRIMARY KEY,             -- "<doc_id>:<item_index>:<line_hash>"
  workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  doc_id          UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  item_index      INTEGER NOT NULL,             -- nth task in the doc, document order
  text            TEXT NOT NULL,
  assignee_user_id UUID NULL REFERENCES users(id) ON DELETE SET NULL,
  checked         BOOLEAN NOT NULL DEFAULT FALSE,
  completed_at    TIMESTAMPTZ NULL,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX doc_tasks_assignee_open ON doc_tasks(workspace_id, assignee_user_id) WHERE completed_at IS NULL;
CREATE INDEX doc_tasks_doc           ON doc_tasks(doc_id);
```

### T2: `TaskStore` trait + Pg impl
- `crates/knot-storage/src/tasks.rs`:
  - `upsert_for_doc(doc_id, items: &[TaskItem])` — `INSERT ... ON CONFLICT DO UPDATE`. Items not present in the new set get marked deleted (or just removed; tasks are derivative).
  - `list_for_assignee(workspace_id, user_id, include_completed: bool)`.
  - `list_for_doc(doc_id)`.

### T3: Markdown task extractor
- `crates/knot-markdown/src/tasks.rs`: walk events, capture `TaskListMarker(bool)` + the inline content of each task item. Resolve leading mention sentinel to `assignee_user_id` (mention extension already serializes to `@<user_id>` per Plan 19's wiring — verify the exact format used).

### T4: Wire to markdown-cache refresh
- `crates/knot-server/src/markdown_cache.rs` (or wherever the cache write happens): after writing markdown, call `task_extractor::extract(...)` and `tasks.upsert_for_doc(...)`.
- Behind a feature flag if the indexing latency becomes a problem; default-on.

### T5: REST endpoint
- `GET /api/workspaces/me/tasks?assignee=<user_id or "me">&include_completed=<bool>`. Returns `[{ id, doc_id, doc_title, text, assignee, checked, completed_at, created_at }]`.
- ACL: filter to docs the requesting user can read (use existing `acl.effective_role`).

### T6: Frontend `/tasks` page
- Route `/tasks` in `routes.tsx`, sidebar entry in `AppShell`.
- Page query: `tasksApi.list({ assignee: 'me' })`.
- UI: groups by doc; each task shows `[ ]` text → click to navigate. Header filter `Mine | All`. Owner gets per-assignee filter dropdown.
- Each task row links to `/doc/<id>?taskIndex=<item_index>`; DocPage reads the query and scrolls the matching `task_item` into view + briefly highlights it.

### T7: Markdown extractor unit tests + integration test through the cache refresh path.

### T8: e2e
- Two users: A creates doc with `- [ ] @bob something`. B opens `/tasks` → sees the item. B checks it → A's `/tasks` refreshes (manual refresh OK; live updates deferred).

### T9: Outcome doc

---

## Open trade-offs
- **Live updates** to `/tasks` (push when someone checks an item somewhere) — deferred. Manual refresh acceptable for v1.
- **Multiple assignees** on one task — keep single-assignee for v1; if users want it, switch to a join table later.
- **Due dates / priorities** — out of scope; would need new attrs on `task_item`.
- **Completed-task history page** — current REST supports `include_completed=true`; UI is one query away.
