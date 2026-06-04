-- Plan 35: task due dates.
--
-- A task can carry an explicit "due by" timestamp lifted from an inline
-- knot://time link in the task content (see knot_markdown::tasks). We
-- store it on doc_tasks so the workspace todo view can sort/group
-- by urgency without re-parsing every doc on every visit.

ALTER TABLE doc_tasks
  ADD COLUMN due_at TIMESTAMPTZ NULL;

-- Listing open tasks for a user sorted by due_at is the dominant
-- query. NULL due_at sorts last via NULLS LAST in the query.
CREATE INDEX doc_tasks_assignee_due
  ON doc_tasks(workspace_id, assignee_user_id, due_at)
  WHERE completed_at IS NULL;
