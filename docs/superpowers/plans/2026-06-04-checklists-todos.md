# Checklists + Assignees Plan (Plan 30)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** GFM-style task lists in the editor. A task item can optionally carry an assignee derived from a leading `@mention`. This plan is *just* the editing primitive — the cross-doc todo view is Plan 31.

**Architecture:**
- Tiptap `@tiptap/extension-task-list` + `@tiptap/extension-task-item`.
- Schema nodes: `task_list` (block, content `task_item+`), `task_item` (block, content `paragraph block*`, attrs `checked: bool` (default false)).
- Assignee is **not** a separate attr — it's the existing `Mention` extension placed as the first inline node of the task item. A small selector reads "if the first inline of a task_item's first paragraph is a Mention, that user is the assignee."
- Markdown via pulldown_cmark's `Options::ENABLE_TASKLISTS` (already enabled in `public.rs`; need to mirror in `from_markdown`).
- Round-trip: `- [ ] @alice Buy milk` and `- [x] done thing`.

---

## Tasks

### T1: Schema — task_list / task_item nodes
- `tools/schema.json`:
  - `task_list` (group `block`, content `task_item+`, isBlock true).
  - `task_item` (content `paragraph block*`, isBlock true, attrs `checked` (bool, default false)).
- Regen.

### T2: knot-markdown — round-trip
- `from_markdown.rs`: on `Tag::List(start=None)` with task-list items (pulldown emits `TaskListMarker(bool)` events at the start of each item), build a `task_list` and per-item `task_item` with `checked` set from the marker.
- `to_markdown.rs`: on `task_list`, emit each `task_item` as `- [ ]` / `- [x] ` followed by the item's inline content.
- Fixture + tests.

### T3: Tiptap extensions
- Add `TaskList`, `TaskItem` to `createExtensions`. Configure `TaskItem` with `nested: true` so subtasks work.
- Schema names must match (`task_list`, `task_item`) — Tiptap's defaults are `taskList`/`taskItem` (camelCase); rename via the extension's `name` field if needed. Likely the existing mismatch will require a wrapper `Node.extend({ name: "task_list" })`.

### T4: Toolbar + slash command
- Toolbar button "Task list" (icon `ListChecks` from Lucide) toggles a list at the cursor. testId `toolbar-task-list`.
- Optional slash command `/task` (only if Plan 27's slash-menu lands first).

### T5: Visual styling
- `web/src/styles/prose.css`: task item rows show a checkbox aligned with the first line, struck-through text on `[x]`, assignee mention chip styled.
- Assignee derivation hook: `useTaskAssignee(taskItemNode)` returns `{ user_id, displayName }` if first inline is a Mention; used later by Plan 31's indexer.

### T6: e2e
- Insert task list, check/uncheck, refresh, persisted.
- Type `@alice` at start of task, assert the mention chip renders.
- Markdown export contains `- [ ] @alice ...`.

### T7: Outcome doc

---

## Open trade-offs
- **Due dates on tasks** — out of scope. Would need its own attr + UI.
- **Recurring tasks** — out of scope.
- **Assignee != mention** (separate "assign" action distinct from mentioning) — keep the simple shape for v1. If users complain about ambiguity, layer a dedicated chip on top later.
