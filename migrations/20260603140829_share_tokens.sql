-- share_tokens
-- Created 2026-06-03

-- Public read-only share links per doc. The token is URL-safe base64 of
-- 24 random bytes (~32 chars). Token IS the auth — anonymous GET /p/<token>
-- looks up the row, checks revoked_at + expires_at, then renders the
-- cached markdown as HTML.
CREATE TABLE share_tokens (
  id           UUID PRIMARY KEY,
  token        TEXT NOT NULL UNIQUE,
  workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  doc_id       UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  expires_at   TIMESTAMPTZ NULL,
  revoked_at   TIMESTAMPTZ NULL,
  created_by   UUID NOT NULL REFERENCES users(id),
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
-- Partial index for the hot path: list active tokens for a doc.
CREATE INDEX share_tokens_doc_active_idx
  ON share_tokens(doc_id) WHERE revoked_at IS NULL;
