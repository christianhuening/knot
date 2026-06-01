-- v0.1 schema. See Foundation spec §5 for design rationale.
--
-- This migration creates all tables defined in v0.1. Subsequent plans
-- add migrations for schema changes. We do NOT split this into multiple
-- migration files because v0.1 is one canonical schema, not an
-- evolution history.

-- Extensions
CREATE EXTENSION IF NOT EXISTS citext;
CREATE EXTENSION IF NOT EXISTS pgcrypto;  -- for gen_random_uuid()

-- ---------------------------------------------------------------------
-- 5.1 Identity & tenancy
-- ---------------------------------------------------------------------

CREATE TABLE workspaces (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    slug       text UNIQUE NOT NULL,
    name       text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    email         citext UNIQUE NOT NULL,
    display_name  text NOT NULL,
    password_hash text NULL,                 -- NULL for OIDC-only users
    oidc_subject  text NULL,
    oidc_issuer   text NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (oidc_issuer, oidc_subject)
);

CREATE TABLE workspace_members (
    workspace_id uuid REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id      uuid REFERENCES users(id) ON DELETE CASCADE,
    role         text NOT NULL CHECK (role IN ('owner','editor','viewer')),
    added_at     timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id)
);

CREATE TABLE sessions (
    id           bytea PRIMARY KEY,           -- 32 random bytes
    user_id      uuid REFERENCES users(id) ON DELETE CASCADE,
    workspace_id uuid REFERENCES workspaces(id) ON DELETE CASCADE,
    created_at   timestamptz NOT NULL DEFAULT now(),
    expires_at   timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    user_agent   text,
    ip           inet
);
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);

-- ---------------------------------------------------------------------
-- 5.2 Document tree
-- ---------------------------------------------------------------------

CREATE TABLE documents (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id uuid NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    parent_id    uuid NULL REFERENCES documents(id) ON DELETE CASCADE,
    title        text NOT NULL DEFAULT 'Untitled',
    sort_key     text NOT NULL,
    icon         text NULL,
    created_by   uuid NOT NULL REFERENCES users(id),
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    archived_at  timestamptz NULL,
    UNIQUE (workspace_id, parent_id, sort_key)
);
CREATE INDEX documents_tree_idx ON documents (workspace_id, parent_id, sort_key);
CREATE INDEX documents_workspace_alive_idx ON documents (workspace_id) WHERE archived_at IS NULL;

-- ---------------------------------------------------------------------
-- 5.3 ACL inheritance
-- ---------------------------------------------------------------------

CREATE TABLE document_grants (
    doc_id     uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    principal  text NOT NULL,        -- 'user:<uuid>' or 'group:<oidc-group>'
    role       text NOT NULL CHECK (role IN ('viewer','editor','owner')),
    inherit    boolean NOT NULL DEFAULT true,
    granted_at timestamptz NOT NULL DEFAULT now(),
    granted_by uuid REFERENCES users(id),
    PRIMARY KEY (doc_id, principal)
);

-- ---------------------------------------------------------------------
-- 5.4 CRDT storage
-- ---------------------------------------------------------------------

CREATE TABLE doc_updates (
    seq          bigserial PRIMARY KEY,
    doc_id       uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    update_bytes bytea NOT NULL,
    by_user_id   uuid NULL REFERENCES users(id),
    created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX doc_updates_by_doc_idx ON doc_updates (doc_id, seq);

CREATE TABLE doc_snapshots (
    doc_id       uuid NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    snapshot_seq bigint NOT NULL,
    state_bytes  bytea NOT NULL,
    state_vector bytea NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (doc_id, snapshot_seq)
);

-- ---------------------------------------------------------------------
-- 5.5 Markdown cache
-- ---------------------------------------------------------------------

CREATE TABLE doc_markdown_cache (
    doc_id          uuid PRIMARY KEY REFERENCES documents(id) ON DELETE CASCADE,
    rendered_at_seq bigint NOT NULL,
    markdown_text   text NOT NULL,
    updated_at      timestamptz NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------
-- 5.6 Audit / activity (skeleton; no UI in v0.1)
-- ---------------------------------------------------------------------

CREATE TABLE audit_events (
    id           bigserial PRIMARY KEY,
    workspace_id uuid NOT NULL,
    actor_id     uuid NULL,
    action       text NOT NULL,
    target_kind  text NOT NULL,
    target_id    uuid NOT NULL,
    data         jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at   timestamptz NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------
-- 5.7 ACL invalidation outbox
-- ---------------------------------------------------------------------

CREATE TABLE acl_invalidations (
    id           bigserial PRIMARY KEY,
    workspace_id uuid NOT NULL,
    doc_id       uuid NOT NULL,
    reason       text NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);
