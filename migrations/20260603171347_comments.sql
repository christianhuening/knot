-- comments
-- Created 2026-06-03

-- Comment threads on docs. Threads are 1-level: parent_id is NULL for the
-- thread root, and equal to thread_id for replies. position_y is a Yjs
-- RelativePosition (base64-encoded by the client, stored as bytea) that
-- pins the thread to a stable point in the editor's text. NULL means the
-- thread is whole-doc.
CREATE TABLE comments (
  id           UUID PRIMARY KEY,
  doc_id       UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  thread_id    UUID NOT NULL,
  parent_id    UUID NULL,
  author_id    UUID NOT NULL REFERENCES users(id),
  body         TEXT NOT NULL CHECK (length(body) > 0 AND length(body) <= 4096),
  position_y   BYTEA NULL,
  anchor_text  TEXT NULL,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  resolved_at  TIMESTAMPTZ NULL,
  deleted_at   TIMESTAMPTZ NULL
);
CREATE INDEX comments_doc_idx    ON comments(doc_id)    WHERE deleted_at IS NULL;
CREATE INDEX comments_thread_idx ON comments(thread_id) WHERE deleted_at IS NULL;

-- Per-user emoji reactions on comments. PK enforces toggle semantics.
CREATE TABLE comment_reactions (
  comment_id   UUID NOT NULL REFERENCES comments(id) ON DELETE CASCADE,
  user_id      UUID NOT NULL REFERENCES users(id),
  emoji        TEXT NOT NULL,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (comment_id, user_id, emoji)
);
CREATE INDEX comment_reactions_user_idx ON comment_reactions(user_id);
