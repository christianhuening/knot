-- blobs
-- Created 2026-06-03

-- Metadata for every blob attached to a doc. The bytes themselves live in
-- either blob_bytes (Postgres backend) or an S3 bucket (S3 backend).
CREATE TABLE blobs (
  id            UUID PRIMARY KEY,
  workspace_id  UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
  doc_id        UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  content_type  TEXT NOT NULL,
  byte_size     BIGINT NOT NULL CHECK (byte_size > 0 AND byte_size <= 10485760),
  sha256        BYTEA NOT NULL,
  original_name TEXT,
  created_by    UUID NOT NULL REFERENCES users(id),
  created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX blobs_doc_idx       ON blobs(doc_id);
CREATE INDEX blobs_workspace_idx ON blobs(workspace_id);

-- Bytes table — Postgres backend only. Separate so metadata stays light.
CREATE TABLE blob_bytes (
  blob_id UUID PRIMARY KEY REFERENCES blobs(id) ON DELETE CASCADE,
  bytes   BYTEA NOT NULL
);
