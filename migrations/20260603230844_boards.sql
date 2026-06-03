-- boards
-- Created 2026-06-03
--
-- Excalidraw-style canvas boards live in their own Yjs document, with their
-- own append-only update log and snapshot ladder mirroring the document tables.
-- A board belongs to a parent document and inherits its ACL — there is no
-- per-board grant table for v0.1.

CREATE TABLE boards (
  id           UUID PRIMARY KEY,
  doc_id       UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  created_by   UUID NOT NULL REFERENCES users(id),
  label        TEXT NULL,
  svg_cached   BYTEA NULL,            -- latest client-uploaded SVG render
  svg_seq      BIGINT NOT NULL DEFAULT 0,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  deleted_at   TIMESTAMPTZ NULL
);
CREATE INDEX boards_doc_idx ON boards(doc_id) WHERE deleted_at IS NULL;

-- Yjs update log: mirrors doc_updates structure so the BoardRoom actor can
-- replay history on boot and append new updates from connected clients.
CREATE TABLE board_updates (
  board_id     UUID NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
  seq          BIGSERIAL NOT NULL,
  bytes        BYTEA NOT NULL,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (board_id, seq)
);

-- Snapshots: mirrors doc_snapshots. Compacts the update log periodically so
-- room boot doesn't need to replay every individual operation.
CREATE TABLE board_snapshots (
  board_id     UUID NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
  snapshot_seq BIGINT NOT NULL,
  state        BYTEA NOT NULL,
  byte_size    BIGINT NOT NULL,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (board_id, snapshot_seq)
);
