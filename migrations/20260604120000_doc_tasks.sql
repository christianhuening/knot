-- Plan 31: workspace todo view.
--
-- Indexes checklist items extracted from each doc's markdown cache so the
-- /tasks page can list "everything assigned to me" without scanning every
-- doc on every render. Idempotent: the indexer upserts the full set for a
-- doc and deletes rows that fell out of the source markdown.

CREATE TABLE doc_tasks (
  -- Stable per-task identity: "<doc_id>:<item_index>" — item_index counts
  -- task items in document order. Re-ordering a list invalidates ids on
  -- purpose so a re-index is straightforward.
  id              TEXT PRIMARY KEY,
  workspace_id    UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  doc_id          UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  item_index      INTEGER NOT NULL,
  text            TEXT NOT NULL,
  assignee_user_id UUID NULL REFERENCES users(id) ON DELETE SET NULL,
  checked         BOOLEAN NOT NULL DEFAULT FALSE,
  completed_at    TIMESTAMPTZ NULL,
  created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Open-tasks-by-assignee is the dominant query.
CREATE INDEX doc_tasks_assignee_open
  ON doc_tasks(workspace_id, assignee_user_id)
  WHERE completed_at IS NULL;

-- "All tasks in this doc" — used by the indexer to delete rows that
-- dropped out of the source markdown.
CREATE INDEX doc_tasks_doc ON doc_tasks(doc_id);
