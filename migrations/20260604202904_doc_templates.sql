-- Plan 36: document templates.
--
-- A template is a regular document with `is_template = true`. Templates
-- participate in the normal ACL system; the only special behavior is
-- that they are filtered out of the main doc tree by default and
-- listed in the "New document" gallery. Creating a doc from a template
-- markdown-clones the source's content into a fresh doc — comments,
-- history, and CRDT lineage are intentionally not carried over.

ALTER TABLE documents
  ADD COLUMN is_template BOOLEAN NOT NULL DEFAULT FALSE;

-- The gallery query lists templates per workspace; partial index keeps
-- it tiny since templates are a small minority of docs.
CREATE INDEX documents_workspace_templates
  ON documents (workspace_id)
  WHERE is_template AND archived_at IS NULL;
