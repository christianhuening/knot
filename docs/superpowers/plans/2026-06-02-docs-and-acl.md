# Documents & ACL — Implementation Plan (Plan 4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the document tree (CRUD + LexoRank-ordered moves), per-document grants, the inheriting ACL resolver with a moka cache and a Postgres LISTEN/NOTIFY invalidation pipeline, and the full `/api/*` HTTP surface from spec §6.1 (workspace + members + documents + grants).

**Architecture:** Three new crates / modules: `knot-storage` grows real `DocStore` + `GrantStore` (replacing the Plan 2 stub) and a `MemberStore` extension. A new `knot-docs` crate hosts the ACL resolver, moka cache, invalidations outbox API, and the LISTEN/NOTIFY listener task. `knot-server` mounts a new `/api/*` subtree with `Csrf` and `RequireSession` layered on top, plus a new `RequireDocRole(min)` middleware for per-document routes. Plan 3 carryover cleanup happens up-front (T1) so the auth surface flows config the right way before this plan extends it.

**Tech Stack:** sqlx 0.8 (incl. existing `ipnetwork`, plus `listen` via `PgListener`), `moka` 0.12 (in-process cache, sync API), tokio LISTEN task, axum 0.7. LexoRank-style ordering implemented as a pure-Rust helper.

**Predecessor:** Plan 3 (Auth, tag candidate `plan-3-complete` at `67935d2`). knot-auth + 3 storage stores + 6 auth endpoints + auth middlewares all live. The `documents`, `document_grants`, `acl_invalidations`, `audit_events`, `workspace_members` tables all exist from the Plan 2 schema migration — no new SQL DDL is required.

**Out of scope for this plan** (each gets its own later plan or is intentionally deferred):
- `GET /api/docs/:id/markdown` / `POST /api/docs/:id/markdown` (Plan 5 — needs the room actor for live Y.Doc → MD round-trip).
- `acl_invalidations` outbox writes from inside the room actor on grant-driven WS close (Plan 5).
- Room-side permission revocation mid-session (close frame 4403) — Plan 5.
- Frontend UI for tree / members / grants — Plans 6-8.
- Helm + image build — Plan 9.

---

## Spec coverage map

What this plan implements from `docs/superpowers/specs/2026-06-01-knot-foundation-design.md`:

| Spec section | Tasks |
|---|---|
| §5.2 Documents table queries | T4 (DocStore: list/create/get/rename/move/archive/restore) |
| §5.3 Document grants + ACL inheritance | T5, T6 |
| §5.6 Audit events (best-effort) | T4, T5, T13, T14 (writes; no UI) |
| §5.7 ACL invalidations outbox | T8 (transactional writes); T9 (listener) |
| §6.1 `/api/workspace` + `/api/workspace/members/*` | T2 |
| §6.1 `/api/docs` + `/api/docs/:id` + move/archive/restore | T12, T13, T14 |
| §6.1 `/api/docs/:id/grants/*` | T15 |
| §7.4 Csrf + RequireSession scoped to `/api/*`; RequireDocRole | T10, T11 |
| §7.5 Permission resolver + moka cache + invalidations | T6, T7, T9 |
| Plan 3 carryover #1: Config through AppState (auto-provision) | T1 |
| Plan 3 carryover #2: OIDC existing-user grant policy gating | T1 |
| Plan 3 carryover #3: Remove `WorkspaceStoreError::NotFound` | T1 |

Deferred to later plans (intentional):
- Markdown I/O endpoints (Plan 5).
- WS close-frame 4403 on revocation (Plan 5).
- All UI (Plans 6-8).

---

## File map

```
knot/
├── Cargo.toml                              (modify) add knot-docs member, moka 0.12, lexorank helper deps
│
├── crates/
│   ├── knot-storage/
│   │   ├── Cargo.toml                      (no change)
│   │   └── src/
│   │       ├── lib.rs                      (modify) re-export DocStore/GrantStore/MemberStore
│   │       ├── doc_store.rs                (rewrite) real PgDocStore + Document type + DocStoreError
│   │       ├── grant_store.rs              (new) GrantStore trait + PgGrantStore + Grant type
│   │       ├── invalidations.rs            (new) helpers to insert acl_invalidations rows (same txn as mutations)
│   │       ├── lexorank.rs                 (new) sort_key generators: between/before/after
│   │       ├── workspace_store.rs          (modify) add list_members/update_role/remove_member; drop NotFound variant
│   │       └── audit.rs                    (new) best-effort audit_event writer
│   │
│   ├── knot-docs/                          (new) ACL resolver + cache + LISTEN/NOTIFY listener
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                      re-exports
│   │       ├── acl.rs                      EffectiveRole + resolve() walking parents + workspace fallback
│   │       ├── cache.rs                    AclCache wrapping moka + the resolver
│   │       └── listener.rs                 PgListener task: LISTEN acl_invalidate → evict cache
│   │
│   └── knot-server/
│       ├── Cargo.toml                      (modify) +knot-docs
│       └── src/
│           ├── lib.rs                      (modify) AppState carries Arc<AclCache>, Arc<Config>; /api router; layer Csrf+RequireSession
│           ├── main.rs                     (modify) spawn ACL listener; wire AclCache into AppState
│           ├── auth/
│           │   └── require_doc_role.rs     (new) RequireDocRole(min) middleware using AclCache
│           └── routes/
│               ├── mod.rs                  (modify) add api submodule
│               └── api/
│                   ├── mod.rs              (new) /api router; mounts workspace/* and docs/*
│                   ├── workspace.rs        (new) GET /api/workspace + members CRUD
│                   ├── docs.rs             (new) list/create/get/patch/move/archive/restore
│                   └── grants.rs           (new) list/put/delete grants
│
└── e2e/
    └── flows/
        ├── workspace.spec.ts               (new) workspace + members CRUD
        └── docs.spec.ts                    (new) doc tree CRUD + grants + ACL inheritance
```

---

## Conventions

- **Audit events:** All mutating doc/grant operations write a best-effort `audit_events` row via `knot_storage::audit::record(pool, …)`. Failure logs warn, doesn't fail the request.
- **Effective roles:** `EffectiveRole` is the user's role for a single (doc_id, user_id) lookup — either an explicit grant up the chain or the workspace role. Returned as `WorkspaceRole` (the same 3-variant enum) since the role set is identical.
- **Authorization codes:** All 403 responses use codes from a small enum: `acl.no_grant`, `doc.not_found`, `workspace.member_not_found`, `workspace.last_owner`, etc. — see each task for the specific codes it emits.
- **Move semantics:** A move = update `parent_id` + `sort_key` in one transaction. Sort keys use the LexoRank scheme from T3.
- **Soft-delete:** `archived_at IS NOT NULL` hides a doc from `/api/docs` list responses and from per-doc reads, but `/api/docs/:id/restore` flips it back.
- **Audit + invalidation ordering:** Both happen inside the same DB transaction as the mutation. Failure to insert into either rolls back the mutation.

---

## Task overview

| # | Title | LOC ≈ |
|---|---|---|
| 1 | Plan 3 carryovers cleanup | 120 |
| 2 | Workspace members store + endpoints | 280 |
| 3 | LexoRank helper | 130 |
| 4 | DocStore (real impl) + audit | 320 |
| 5 | GrantStore | 220 |
| 6 | ACL resolver (knot-docs crate) | 250 |
| 7 | ACL cache (moka) | 140 |
| 8 | ACL invalidations outbox writers | 70 |
| 9 | ACL NOTIFY listener task | 180 |
| 10 | RequireDocRole middleware | 110 |
| 11 | /api router + CSRF + RequireSession layering | 90 |
| 12 | GET /api/docs + POST /api/docs + GET /api/docs/:id | 240 |
| 13 | PATCH /api/docs/:id + POST /api/docs/:id/move | 200 |
| 14 | DELETE + POST /api/docs/:id/restore | 130 |
| 15 | Grants endpoints | 230 |
| 16 | e2e — workspace + docs + grants flow | 200 |

---

## Task 1: Plan 3 carryovers cleanup

**Files:**
- Modify: `crates/knot-server/src/lib.rs` — AppState gets `pub config: Arc<knot_config::Config>` field
- Modify: `crates/knot-server/src/main.rs` — pass cfg as `Arc<Config>`
- Modify: `crates/knot-server/src/routes/auth/oidc.rs` — read auto-provision policy from `state.config`, gate existing-user grant
- Modify: `crates/knot-storage/src/workspace_store.rs` — drop `WorkspaceStoreError::NotFound`

- [ ] **Step 1: AppState carries Arc<Config>**

Edit `crates/knot-server/src/lib.rs`. Add to imports:

```rust
use std::sync::Arc;
use knot_config::Config;
```

Add to `AppState`:

```rust
    pub config: Arc<Config>,
```

In `in_memory()`:

```rust
            config: Arc::new(Config::default()),
```

In `with_pool(pool)`:

```rust
            config: Arc::new(Config::default()),
```

Edit `crates/knot-server/src/main.rs`. After loading config, wrap in Arc:

```rust
    let cfg = Arc::new(cfg);
```

In the `match pool { Some(p) => { … } }` block where you populate state fields, replace `cfg.foo.clone()` references with `cfg.foo.clone()` (already correct since the closure borrows `cfg`) and add:

```rust
            s.config = cfg.clone();
```

Pass `cfg.clone()` into `run_server`'s reused references where needed.

- [ ] **Step 2: OIDC route reads config instead of env**

Edit `crates/knot-server/src/routes/auth/oidc.rs`. In `auto_provision`, replace `std::env::var("KNOT_OIDC_AUTO_PROVISION").unwrap_or_else(|_| "off".into())` with `state.config.oidc_auto_provision.clone()`. Similarly:

```rust
    let allow = match policy.as_str() {
        "always" => true,
        "domain" => {
            let domains = &state.config.oidc_allowed_domains;
            let user_domain = id.email.split('@').nth(1).unwrap_or("");
            domains
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .any(|d| d == user_domain)
        }
        "group" => {
            let mapping = &state.config.oidc_role_from_groups;
            let parsed: HashMap<String, String> = serde_json::from_str(mapping).unwrap_or_default();
            id.groups.iter().any(|g| parsed.contains_key(g))
        }
        _ => false,
    };
```

Similarly in the post-create "group" policy branch, read `state.config.oidc_role_from_groups`.

- [ ] **Step 3: Gate existing-user workspace add on auto-provision policy**

In `oidc.rs` `callback`, find the existing-user `add_member` block (currently unconditional):

```rust
    if workspaces
        .get_member_role(ws.id, user.id)
        .await
        .ok()
        .flatten()
        .is_none()
    {
        if let Err(e) = workspaces
            .add_member(ws.id, user.id, knot_storage::WorkspaceRole::Viewer)
            .await
        {
            tracing::error!(error=?e, "oidc add_member");
            return internal();
        }
    }
```

Replace with:

```rust
    if workspaces
        .get_member_role(ws.id, user.id)
        .await
        .ok()
        .flatten()
        .is_none()
    {
        if state.config.oidc_auto_provision == "off" {
            return err(
                StatusCode::FORBIDDEN,
                "auth.oidc.not_provisioned",
                "existing user not auto-provisioned",
            );
        }
        if let Err(e) = workspaces
            .add_member(ws.id, user.id, knot_storage::WorkspaceRole::Viewer)
            .await
        {
            tracing::error!(error=?e, "oidc add_member");
            return internal();
        }
    }
```

Rename the local `err` helper to `json_err` if needed for consistency — the existing oidc.rs already aliases.

- [ ] **Step 4: Drop WorkspaceStoreError::NotFound**

Edit `crates/knot-storage/src/workspace_store.rs`. Remove the `NotFound` variant from `WorkspaceStoreError`. It's defined but never constructed. Confirm with `grep -rn 'WorkspaceStoreError::NotFound' crates/` — should produce zero hits after the change.

- [ ] **Step 5: Verify**

```bash
cd /home/nik/Development/knot
cargo build --workspace
cargo test -p knot-server -p knot-storage
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: green. No test regressions from the OIDC policy gating because there are no OIDC e2e tests at the live-IdP level.

- [ ] **Step 6: Commit**

```bash
git add crates/knot-server/ crates/knot-storage/
git commit -m "refactor: thread Config through AppState; gate OIDC existing-user; drop WorkspaceStoreError::NotFound"
```

---

## Task 2: Workspace members store + endpoints

**Files:**
- Modify: `crates/knot-storage/src/workspace_store.rs` — add `list_members`, `update_role`, `remove_member`; add `Member` row type
- Create: `crates/knot-server/src/routes/api/mod.rs` (skeleton)
- Create: `crates/knot-server/src/routes/api/workspace.rs`
- Modify: `crates/knot-server/src/routes/mod.rs` — add `pub mod api;`
- Modify: `crates/knot-server/src/lib.rs` — mount `routes::api::router()` (Task 11 will layer Csrf/RequireSession; T2 leaves the router unauthenticated for now)
- Create: `crates/knot-storage/tests/workspace_members.rs`

- [ ] **Step 1: Extend WorkspaceStore trait + impl**

Edit `crates/knot-storage/src/workspace_store.rs`. Add a new `Member` struct:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub role: WorkspaceRole,
    pub added_at: DateTime<Utc>,
}
```

Add to the trait:

```rust
    async fn list_members(&self, workspace_id: Uuid) -> Result<Vec<Member>, WorkspaceStoreError>;
    async fn update_role(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        role: WorkspaceRole,
    ) -> Result<(), WorkspaceStoreError>;
    async fn remove_member(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), WorkspaceStoreError>;
    async fn count_owners(&self, workspace_id: Uuid) -> Result<i64, WorkspaceStoreError>;
```

Add to `impl WorkspaceStore for PgWorkspaceStore`:

```rust
    async fn list_members(&self, workspace_id: Uuid) -> Result<Vec<Member>, WorkspaceStoreError> {
        let rows = sqlx::query_as::<_, (Uuid, String, String, String, DateTime<Utc>)>(
            "SELECT wm.user_id, u.email::text, u.display_name, wm.role, wm.added_at
             FROM workspace_members wm
             JOIN users u ON u.id = wm.user_id
             WHERE wm.workspace_id = $1
             ORDER BY wm.added_at",
        )
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(uid, email, name, role, added)| {
                Ok(Member {
                    user_id: uid,
                    email,
                    display_name: name,
                    role: WorkspaceRole::parse(&role)
                        .ok_or_else(|| WorkspaceStoreError::InvalidRole(role))?,
                    added_at: added,
                })
            })
            .collect()
    }

    async fn update_role(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        role: WorkspaceRole,
    ) -> Result<(), WorkspaceStoreError> {
        sqlx::query(
            "UPDATE workspace_members SET role = $3
             WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(role.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn remove_member(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), WorkspaceStoreError> {
        sqlx::query(
            "DELETE FROM workspace_members WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(workspace_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn count_owners(&self, workspace_id: Uuid) -> Result<i64, WorkspaceStoreError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM workspace_members
             WHERE workspace_id = $1 AND role = 'owner'",
        )
        .bind(workspace_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(n)
    }
```

Re-export `Member` from `lib.rs`:

```rust
pub use workspace_store::{
    Member, PgWorkspaceStore, Workspace, WorkspaceRole, WorkspaceStore, WorkspaceStoreError,
};
```

- [ ] **Step 2: Integration tests**

Create `crates/knot-storage/tests/workspace_members.rs`:

```rust
use knot_storage::{PgUserStore, PgWorkspaceStore, UserStore, WorkspaceRole, WorkspaceStore};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

async fn setup() -> (PgWorkspaceStore, PgUserStore, uuid::Uuid) {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    std::mem::forget(container);

    let ws = PgWorkspaceStore::new(pool.clone());
    let users = PgUserStore::new(pool);
    let w = ws.create("default", "Workspace").await.unwrap();
    (ws, users, w.id)
}

#[tokio::test(flavor = "multi_thread")]
async fn list_update_remove_members() {
    let (ws, users, ws_id) = setup().await;
    let alice = users.create_local("alice@x.test", "Alice", "$h$").await.unwrap();
    let bob = users.create_local("bob@x.test", "Bob", "$h$").await.unwrap();
    ws.add_member(ws_id, alice.id, WorkspaceRole::Owner).await.unwrap();
    ws.add_member(ws_id, bob.id, WorkspaceRole::Viewer).await.unwrap();

    let members = ws.list_members(ws_id).await.unwrap();
    assert_eq!(members.len(), 2);
    assert!(members.iter().any(|m| m.email == "alice@x.test" && m.role == WorkspaceRole::Owner));

    ws.update_role(ws_id, bob.id, WorkspaceRole::Editor).await.unwrap();
    let role = ws.get_member_role(ws_id, bob.id).await.unwrap();
    assert_eq!(role, Some(WorkspaceRole::Editor));

    ws.remove_member(ws_id, bob.id).await.unwrap();
    assert_eq!(ws.list_members(ws_id).await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn count_owners() {
    let (ws, users, ws_id) = setup().await;
    let a = users.create_local("a@x.test", "A", "$h$").await.unwrap();
    let b = users.create_local("b@x.test", "B", "$h$").await.unwrap();
    ws.add_member(ws_id, a.id, WorkspaceRole::Owner).await.unwrap();
    ws.add_member(ws_id, b.id, WorkspaceRole::Viewer).await.unwrap();
    assert_eq!(ws.count_owners(ws_id).await.unwrap(), 1);
    ws.update_role(ws_id, b.id, WorkspaceRole::Owner).await.unwrap();
    assert_eq!(ws.count_owners(ws_id).await.unwrap(), 2);
}
```

- [ ] **Step 3: /api router skeleton**

Create `crates/knot-server/src/routes/api/mod.rs`:

```rust
//! `/api/*` routes. Auth + CSRF middlewares are layered here (T11).

use axum::Router;

use crate::AppState;

pub mod workspace;

pub fn router() -> Router<AppState> {
    Router::new().merge(workspace::router())
}
```

Edit `crates/knot-server/src/routes/mod.rs` — add `pub mod api;`.

Edit `crates/knot-server/src/lib.rs` `router_with_state` — merge it (no auth layering yet; T11 adds that):

```rust
    let mut r = Router::new()
        .route("/collab/:doc_id", get(collab_upgrade))
        .merge(routes::health::router())
        .merge(routes::auth::router())
        .merge(routes::api::router());
```

- [ ] **Step 4: Workspace endpoints**

Create `crates/knot-server/src/routes/api/workspace.rs`:

```rust
//! GET /api/workspace — workspace + your membership
//! GET /api/workspace/members
//! POST /api/workspace/members        body: {email, role}
//! PATCH /api/workspace/members/:id   body: {role}
//! DELETE /api/workspace/members/:id

use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use knot_storage::WorkspaceRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthContext;
use crate::http_error::json_err;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/workspace", get(get_workspace))
        .route("/api/workspace/members", get(list_members).post(invite_member))
        .route(
            "/api/workspace/members/:id",
            patch(change_role).delete(remove_member),
        )
}

#[derive(Serialize)]
struct WorkspaceResponse {
    id: String,
    slug: String,
    name: String,
    role: String,
}

async fn get_workspace(State(state): State<AppState>, req: Request) -> Response {
    let Some(ctx) = ctx(&req) else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(workspaces) = state.workspaces.clone() else { return internal() };
    let ws = match workspaces.get_singleton().await {
        Ok(Some(w)) => w,
        _ => return internal(),
    };
    Json(WorkspaceResponse {
        id: ws.id.to_string(),
        slug: ws.slug,
        name: ws.name,
        role: ctx.role.as_str().into(),
    })
    .into_response()
}

#[derive(Serialize)]
struct MemberResponse {
    user_id: String,
    email: String,
    display_name: String,
    role: String,
}

async fn list_members(State(state): State<AppState>, req: Request) -> Response {
    let Some(_ctx) = ctx(&req) else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(workspaces) = state.workspaces.clone() else { return internal() };
    let ws = match workspaces.get_singleton().await {
        Ok(Some(w)) => w,
        _ => return internal(),
    };
    match workspaces.list_members(ws.id).await {
        Ok(members) => Json(
            members
                .into_iter()
                .map(|m| MemberResponse {
                    user_id: m.user_id.to_string(),
                    email: m.email,
                    display_name: m.display_name,
                    role: m.role.as_str().into(),
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => {
            tracing::error!(error=?e, "list_members");
            internal()
        }
    }
}

#[derive(Deserialize)]
struct InviteRequest {
    email: String,
    role: String,
}

async fn invite_member(
    State(state): State<AppState>,
    req: Request,
) -> Response {
    let Some(ctx) = ctx(&req) else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    if ctx.role != WorkspaceRole::Owner {
        return json_err(StatusCode::FORBIDDEN, "acl.owner_required", "");
    }
    let Ok(body) = read_json::<InviteRequest>(req).await else {
        return json_err(StatusCode::BAD_REQUEST, "bad_request", "");
    };
    let Some(role) = WorkspaceRole::parse(&body.role) else {
        return json_err(StatusCode::UNPROCESSABLE_ENTITY, "workspace.invalid_role", "");
    };
    let Some(users) = state.users.clone() else { return internal() };
    let Some(workspaces) = state.workspaces.clone() else { return internal() };

    let user = match users.find_by_email(&body.email).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return json_err(
                StatusCode::NOT_FOUND,
                "workspace.user_not_found",
                "user must exist before invite (v0.1 has no email-invite flow)",
            );
        }
        Err(e) => {
            tracing::error!(error=?e, "invite lookup");
            return internal();
        }
    };
    let ws = match workspaces.get_singleton().await {
        Ok(Some(w)) => w,
        _ => return internal(),
    };
    if let Err(e) = workspaces.add_member(ws.id, user.id, role).await {
        match e {
            knot_storage::WorkspaceStoreError::Sqlx(ref s) if is_unique_violation(s) => {
                return json_err(StatusCode::CONFLICT, "workspace.already_member", "");
            }
            _ => {
                tracing::error!(error=?e, "invite add_member");
                return internal();
            }
        }
    }
    StatusCode::CREATED.into_response()
}

#[derive(Deserialize)]
struct ChangeRoleRequest {
    role: String,
}

async fn change_role(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    req: Request,
) -> Response {
    let Some(ctx) = ctx(&req) else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    if ctx.role != WorkspaceRole::Owner {
        return json_err(StatusCode::FORBIDDEN, "acl.owner_required", "");
    }
    let Ok(body) = read_json::<ChangeRoleRequest>(req).await else {
        return json_err(StatusCode::BAD_REQUEST, "bad_request", "");
    };
    let Some(new_role) = WorkspaceRole::parse(&body.role) else {
        return json_err(StatusCode::UNPROCESSABLE_ENTITY, "workspace.invalid_role", "");
    };
    let Some(workspaces) = state.workspaces.clone() else { return internal() };
    let ws = match workspaces.get_singleton().await {
        Ok(Some(w)) => w,
        _ => return internal(),
    };

    // Prevent demoting the last owner.
    if new_role != WorkspaceRole::Owner {
        let current = workspaces.get_member_role(ws.id, user_id).await.ok().flatten();
        if current == Some(WorkspaceRole::Owner) {
            let owners = workspaces.count_owners(ws.id).await.unwrap_or(0);
            if owners <= 1 {
                return json_err(
                    StatusCode::CONFLICT,
                    "workspace.last_owner",
                    "cannot demote the last owner",
                );
            }
        }
    }

    if let Err(e) = workspaces.update_role(ws.id, user_id, new_role).await {
        tracing::error!(error=?e, "update_role");
        return internal();
    }
    StatusCode::NO_CONTENT.into_response()
}

async fn remove_member(
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    req: Request,
) -> Response {
    let Some(ctx) = ctx(&req) else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    if ctx.role != WorkspaceRole::Owner {
        return json_err(StatusCode::FORBIDDEN, "acl.owner_required", "");
    }
    let Some(workspaces) = state.workspaces.clone() else { return internal() };
    let ws = match workspaces.get_singleton().await {
        Ok(Some(w)) => w,
        _ => return internal(),
    };
    let current = workspaces.get_member_role(ws.id, user_id).await.ok().flatten();
    if current == Some(WorkspaceRole::Owner) {
        let owners = workspaces.count_owners(ws.id).await.unwrap_or(0);
        if owners <= 1 {
            return json_err(
                StatusCode::CONFLICT,
                "workspace.last_owner",
                "cannot remove the last owner",
            );
        }
    }
    if let Err(e) = workspaces.remove_member(ws.id, user_id).await {
        tracing::error!(error=?e, "remove_member");
        return internal();
    }
    StatusCode::NO_CONTENT.into_response()
}

fn ctx(req: &Request) -> Option<AuthContext> {
    req.extensions().get::<AuthContext>().cloned()
}

async fn read_json<T: serde::de::DeserializeOwned>(req: Request) -> Result<T, ()> {
    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
        .await
        .map_err(|_| ())?;
    serde_json::from_slice(&bytes).map_err(|_| ())
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.is_unique_violation())
}

fn internal() -> Response {
    json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "")
}
```

- [ ] **Step 4 verification**

```bash
cd /home/nik/Development/knot
cargo build --workspace
cargo test -p knot-storage --test workspace_members
cargo clippy -p knot-server --all-targets --all-features -- -D warnings
```

Expected: 2 new storage tests pass; clippy clean; server builds.

- [ ] **Step 5: Commit**

```bash
git add crates/
git commit -m "feat(workspace): list/update/remove members + /api/workspace endpoints"
```

---

## Task 3: LexoRank helper

**Files:**
- Create: `crates/knot-storage/src/lexorank.rs`
- Modify: `crates/knot-storage/src/lib.rs` — `pub mod lexorank;` + re-export `sort_key_between`

LexoRank generates a sort_key that lives between two existing keys. For knot v0.1 we use a single-bucket scheme (no bucket prefix) — the keys are pure base-36 strings using `0-9a-z`. Between `"m"` and `"n"` is `"ma"`-`"mz"`; between `"m"` and `"mm"` is `"mb"`-`"ml"` or `"m1"`-`"ml"`.

- [ ] **Step 1: Write failing tests**

Create `crates/knot-storage/src/lexorank.rs`:

```rust
//! LexoRank-style sort_key generation. Single-bucket base-36 (0-9a-z).
//!
//! Properties:
//! - `between(None, None) -> "m"` (start with a middle anchor so future
//!   inserts on both sides remain cheap).
//! - `between(Some(a), None)` returns something > a.
//! - `between(None, Some(b))` returns something < b.
//! - `between(Some(a), Some(b))` where a < b returns a key in (a, b).
//! - Returned keys never end in '0' (so we can always append a digit).

const MIN: char = '0';
const MAX: char = 'z';
const MID: char = 'm';

fn next(c: char) -> char {
    let v = (c as u8) + 1;
    char::from(v.min(MAX as u8))
}
fn prev(c: char) -> char {
    let v = (c as u8).saturating_sub(1);
    char::from(v.max(MIN as u8))
}
fn mid(a: char, b: char) -> char {
    let av = a as u8;
    let bv = b as u8;
    char::from(((av as u16 + bv as u16) / 2) as u8)
}

fn is_base36(s: &str) -> bool {
    s.chars().all(|c| matches!(c, '0'..='9' | 'a'..='z'))
}

pub fn between(a: Option<&str>, b: Option<&str>) -> String {
    match (a, b) {
        (None, None) => MID.to_string(),
        (Some(a), None) => append_or_grow(a),
        (None, Some(b)) => decrement(b),
        (Some(a), Some(b)) => {
            assert!(a < b, "between: a must be < b ({a} >= {b})");
            assert!(is_base36(a) && is_base36(b), "base36 only");
            interpolate(a, b)
        }
    }
}

fn append_or_grow(a: &str) -> String {
    // Append 'm' to a; if a ends in 'z', extend differently.
    let mut s = a.to_string();
    let last = s.chars().last().unwrap_or(MIN);
    if last < MAX {
        s.push(mid(last, MAX));
    } else {
        s.push(MID);
    }
    s
}

fn decrement(b: &str) -> String {
    // Build a key < b. Replace the first non-min char with its mid-down.
    let bytes = b.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() + 1);
    for &c in bytes {
        let ch = c as char;
        if ch > MIN {
            out.push(mid(MIN, ch) as u8);
            return String::from_utf8(out).unwrap();
        }
        out.push(c);
    }
    // All zeros; fall back to MID.
    out.push(MID as u8);
    String::from_utf8(out).unwrap()
}

fn interpolate(a: &str, b: &str) -> String {
    // Walk both keys character-by-character. As long as they agree, copy
    // the agreed prefix into `out`. At the first divergence at position p:
    //   if b[p] > a[p] + 1, return out + mid(a[p], b[p]).
    //   else, copy a[p], advance p, and look for the next char of `a` that
    //   can grow toward MAX (since at every subsequent position a < b
    //   implicitly because `b` no longer constrains us).
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < a_chars.len() && i < b_chars.len() && a_chars[i] == b_chars[i] {
        out.push(a_chars[i]);
        i += 1;
    }
    let ac = a_chars.get(i).copied().unwrap_or(MIN);
    let bc = b_chars.get(i).copied().unwrap_or(MAX);
    if (bc as u8) > (ac as u8) + 1 {
        out.push(mid(ac, bc));
        return out;
    }
    // Adjacent at i. Take a[i], then grow from there.
    out.push(ac);
    i += 1;
    loop {
        let ac = a_chars.get(i).copied().unwrap_or(MIN);
        if ac < MAX {
            out.push(mid(ac, MAX));
            return out;
        }
        out.push(ac); // = MAX
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_between(a: Option<&str>, b: Option<&str>) {
        let r = between(a, b);
        if let Some(av) = a { assert!(av < r.as_str(), "expected {av} < {r}"); }
        if let Some(bv) = b { assert!(r.as_str() < bv, "expected {r} < {bv}"); }
        assert!(is_base36(&r), "{r} not base36");
    }

    #[test]
    fn empty_returns_middle() {
        assert_eq!(between(None, None), "m");
    }

    #[test]
    fn after_only_extends_or_grows() {
        check_between(Some("m"), None);
        check_between(Some("z"), None);
    }

    #[test]
    fn before_only_decrements() {
        check_between(None, Some("m"));
        check_between(None, Some("a"));
    }

    #[test]
    fn adjacent_chars_descend_into_suffix() {
        check_between(Some("a"), Some("b"));
        check_between(Some("m"), Some("n"));
    }

    #[test]
    fn distant_chars_pick_midpoint() {
        let r = between(Some("a"), Some("z"));
        assert!("a" < r.as_str() && r.as_str() < "z");
    }

    #[test]
    fn many_inserts_between_two_anchors_stay_monotone() {
        let mut a = "a".to_string();
        let b = "z";
        for _ in 0..50 {
            let next = between(Some(&a), Some(b));
            assert!(a.as_str() < next.as_str() && next.as_str() < b);
            a = next;
        }
    }
}
```

- [ ] **Step 2: lib.rs**

Edit `crates/knot-storage/src/lib.rs` — `pub mod lexorank;` and `pub use lexorank::between as sort_key_between;`.

- [ ] **Step 3: Run tests**

```bash
cargo test -p knot-storage --lib lexorank
```

Expected: 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/knot-storage/
git commit -m "feat(knot-storage): LexoRank-style sort_key helper"
```

---

## Task 4: DocStore + audit

**Files:**
- Rewrite: `crates/knot-storage/src/doc_store.rs`
- Create: `crates/knot-storage/src/audit.rs`
- Create: `crates/knot-storage/src/invalidations.rs` (stub — T8 fills queries)
- Modify: `crates/knot-storage/src/lib.rs` — re-export `Document`, `DocStore` impl, audit
- Create: `crates/knot-storage/tests/documents.rs`

- [ ] **Step 1: audit helper**

Create `crates/knot-storage/src/audit.rs`:

```rust
//! Best-effort audit_events writer. Failures are logged + swallowed.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub async fn record(
    pool: &PgPool,
    workspace_id: Uuid,
    actor: Option<Uuid>,
    action: &str,
    target_kind: &str,
    target_id: Uuid,
) {
    let result = sqlx::query(
        "INSERT INTO audit_events (workspace_id, actor_id, action, target_kind, target_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(workspace_id)
    .bind(actor)
    .bind(action)
    .bind(target_kind)
    .bind(target_id)
    .execute(pool)
    .await;
    if let Err(e) = result {
        tracing::warn!(error=?e, action, "audit write failed (best-effort)");
    }
}

/// Same as `record` but accepts an in-flight transaction so the audit row
/// is committed alongside its mutation.
pub async fn record_in_tx(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    actor: Option<Uuid>,
    action: &str,
    target_kind: &str,
    target_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO audit_events (workspace_id, actor_id, action, target_kind, target_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(workspace_id)
    .bind(actor)
    .bind(action)
    .bind(target_kind)
    .bind(target_id)
    .execute(&mut *tx)
    .await?;
    Ok(())
}
```

- [ ] **Step 2: invalidations stub**

Create `crates/knot-storage/src/invalidations.rs`:

```rust
//! ACL invalidations outbox. Rows written in the same transaction as the
//! mutation; consumed by the listener in knot-docs.

use sqlx::PgConnection;
use uuid::Uuid;

pub async fn record_in_tx(
    tx: &mut PgConnection,
    workspace_id: Uuid,
    doc_id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO acl_invalidations (workspace_id, doc_id, reason)
         VALUES ($1, $2, $3)",
    )
    .bind(workspace_id)
    .bind(doc_id)
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    // Notify listeners. Payload = doc_id text so listener can target evictions.
    sqlx::query(&format!("NOTIFY acl_invalidate, '{}'", doc_id))
        .execute(&mut *tx)
        .await?;
    Ok(())
}
```

- [ ] **Step 3: DocStore replacement**

Replace `crates/knot-storage/src/doc_store.rs`:

```rust
//! Document storage — CRUD + tree ops.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::audit;
use crate::invalidations;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub title: String,
    pub sort_key: String,
    pub icon: Option<String>,
    pub created_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Error)]
pub enum DocStoreError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("not found")]
    NotFound,
    #[error("conflict")]
    Conflict,
}

#[async_trait]
pub trait DocStore: Send + Sync + 'static {
    async fn list_alive(&self, workspace_id: Uuid) -> Result<Vec<Document>, DocStoreError>;
    async fn get(&self, doc_id: Uuid) -> Result<Option<Document>, DocStoreError>;
    async fn create(
        &self,
        workspace_id: Uuid,
        parent_id: Option<Uuid>,
        title: &str,
        sort_key: &str,
        created_by: Uuid,
    ) -> Result<Document, DocStoreError>;
    async fn rename(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        actor: Uuid,
        title: &str,
        icon: Option<&str>,
    ) -> Result<Document, DocStoreError>;
    async fn move_to(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        actor: Uuid,
        parent_id: Option<Uuid>,
        sort_key: &str,
    ) -> Result<Document, DocStoreError>;
    async fn archive(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        actor: Uuid,
    ) -> Result<(), DocStoreError>;
    async fn restore(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        actor: Uuid,
    ) -> Result<(), DocStoreError>;
    /// Returns siblings under `parent_id` in sort order. Used to compute
    /// LexoRank neighbours for create/move with `after_id`/`before_id`.
    async fn siblings(
        &self,
        workspace_id: Uuid,
        parent_id: Option<Uuid>,
    ) -> Result<Vec<Document>, DocStoreError>;
}

#[derive(Clone)]
pub struct PgDocStore {
    pool: PgPool,
}

impl PgDocStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

type DocRow = (
    Uuid, Uuid, Option<Uuid>, String, String, Option<String>, Uuid,
    DateTime<Utc>, DateTime<Utc>, Option<DateTime<Utc>>,
);
fn doc_from_row(r: DocRow) -> Document {
    Document {
        id: r.0, workspace_id: r.1, parent_id: r.2, title: r.3, sort_key: r.4,
        icon: r.5, created_by: r.6, created_at: r.7, updated_at: r.8, archived_at: r.9,
    }
}
const COLS: &str =
    "id, workspace_id, parent_id, title, sort_key, icon, created_by, created_at, updated_at, archived_at";

#[async_trait]
impl DocStore for PgDocStore {
    async fn list_alive(&self, workspace_id: Uuid) -> Result<Vec<Document>, DocStoreError> {
        let rows = sqlx::query_as::<_, DocRow>(&format!(
            "SELECT {COLS} FROM documents
             WHERE workspace_id = $1 AND archived_at IS NULL
             ORDER BY parent_id NULLS FIRST, sort_key"
        ))
        .bind(workspace_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(doc_from_row).collect())
    }

    async fn get(&self, doc_id: Uuid) -> Result<Option<Document>, DocStoreError> {
        let row = sqlx::query_as::<_, DocRow>(&format!(
            "SELECT {COLS} FROM documents WHERE id = $1"
        ))
        .bind(doc_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(doc_from_row))
    }

    async fn create(
        &self,
        workspace_id: Uuid,
        parent_id: Option<Uuid>,
        title: &str,
        sort_key: &str,
        created_by: Uuid,
    ) -> Result<Document, DocStoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, DocRow>(&format!(
            "INSERT INTO documents (workspace_id, parent_id, title, sort_key, created_by)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING {COLS}"
        ))
        .bind(workspace_id)
        .bind(parent_id)
        .bind(title)
        .bind(sort_key)
        .bind(created_by)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_unique)?;
        let doc = doc_from_row(row);
        audit::record_in_tx(&mut tx, workspace_id, Some(created_by), "doc.create", "doc", doc.id).await?;
        invalidations::record_in_tx(&mut tx, workspace_id, doc.id, "create").await?;
        tx.commit().await?;
        Ok(doc)
    }

    async fn rename(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        actor: Uuid,
        title: &str,
        icon: Option<&str>,
    ) -> Result<Document, DocStoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, DocRow>(&format!(
            "UPDATE documents SET title = $3, icon = COALESCE($4, icon), updated_at = now()
             WHERE workspace_id = $1 AND id = $2
             RETURNING {COLS}"
        ))
        .bind(workspace_id)
        .bind(doc_id)
        .bind(title)
        .bind(icon)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DocStoreError::NotFound)?;
        let doc = doc_from_row(row);
        audit::record_in_tx(&mut tx, workspace_id, Some(actor), "doc.rename", "doc", doc.id).await?;
        tx.commit().await?;
        Ok(doc)
    }

    async fn move_to(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        actor: Uuid,
        parent_id: Option<Uuid>,
        sort_key: &str,
    ) -> Result<Document, DocStoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query_as::<_, DocRow>(&format!(
            "UPDATE documents SET parent_id = $3, sort_key = $4, updated_at = now()
             WHERE workspace_id = $1 AND id = $2
             RETURNING {COLS}"
        ))
        .bind(workspace_id)
        .bind(doc_id)
        .bind(parent_id)
        .bind(sort_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_unique)?
        .ok_or(DocStoreError::NotFound)?;
        let doc = doc_from_row(row);
        audit::record_in_tx(&mut tx, workspace_id, Some(actor), "doc.move", "doc", doc.id).await?;
        invalidations::record_in_tx(&mut tx, workspace_id, doc.id, "tree-move").await?;
        tx.commit().await?;
        Ok(doc)
    }

    async fn archive(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        actor: Uuid,
    ) -> Result<(), DocStoreError> {
        let mut tx = self.pool.begin().await?;
        let n = sqlx::query(
            "UPDATE documents SET archived_at = now()
             WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL",
        )
        .bind(workspace_id)
        .bind(doc_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if n == 0 { return Err(DocStoreError::NotFound); }
        audit::record_in_tx(&mut tx, workspace_id, Some(actor), "doc.archive", "doc", doc_id).await?;
        invalidations::record_in_tx(&mut tx, workspace_id, doc_id, "archive").await?;
        tx.commit().await?;
        Ok(())
    }

    async fn restore(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        actor: Uuid,
    ) -> Result<(), DocStoreError> {
        let mut tx = self.pool.begin().await?;
        let n = sqlx::query(
            "UPDATE documents SET archived_at = NULL
             WHERE workspace_id = $1 AND id = $2 AND archived_at IS NOT NULL",
        )
        .bind(workspace_id)
        .bind(doc_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if n == 0 { return Err(DocStoreError::NotFound); }
        audit::record_in_tx(&mut tx, workspace_id, Some(actor), "doc.restore", "doc", doc_id).await?;
        invalidations::record_in_tx(&mut tx, workspace_id, doc_id, "restore").await?;
        tx.commit().await?;
        Ok(())
    }

    async fn siblings(
        &self,
        workspace_id: Uuid,
        parent_id: Option<Uuid>,
    ) -> Result<Vec<Document>, DocStoreError> {
        let rows = sqlx::query_as::<_, DocRow>(&format!(
            "SELECT {COLS} FROM documents
             WHERE workspace_id = $1 AND parent_id IS NOT DISTINCT FROM $2
                   AND archived_at IS NULL
             ORDER BY sort_key"
        ))
        .bind(workspace_id)
        .bind(parent_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(doc_from_row).collect())
    }
}

fn map_unique(e: sqlx::Error) -> DocStoreError {
    match e {
        sqlx::Error::Database(ref db) if db.is_unique_violation() => DocStoreError::Conflict,
        e => DocStoreError::Sqlx(e),
    }
}
```

- [ ] **Step 4: lib.rs re-exports**

Edit `crates/knot-storage/src/lib.rs`:

```rust
pub mod audit;
pub mod invalidations;
pub mod doc_store;
pub mod lexorank;
// ...
pub use doc_store::{Document, DocStore, DocStoreError, PgDocStore};
```

- [ ] **Step 5: Integration tests**

Create `crates/knot-storage/tests/documents.rs`:

```rust
use knot_storage::{
    PgDocStore, PgUserStore, PgWorkspaceStore, UserStore, WorkspaceRole, WorkspaceStore,
    DocStore, sort_key_between,
};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
use uuid::Uuid;

async fn setup() -> (PgDocStore, Uuid, Uuid) {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    std::mem::forget(container);

    let ws = PgWorkspaceStore::new(pool.clone()).create("default", "W").await.unwrap();
    let users = PgUserStore::new(pool.clone());
    let u = users.create_local("a@x.test", "A", "$h$").await.unwrap();
    PgWorkspaceStore::new(pool.clone()).add_member(ws.id, u.id, WorkspaceRole::Owner).await.unwrap();
    (PgDocStore::new(pool), ws.id, u.id)
}

#[tokio::test(flavor = "multi_thread")]
async fn create_get_list_lifecycle() {
    let (store, ws, user) = setup().await;
    let sk = sort_key_between(None, None);
    let doc = store.create(ws, None, "Hello", &sk, user).await.unwrap();
    assert_eq!(doc.title, "Hello");
    assert_eq!(doc.workspace_id, ws);
    let got = store.get(doc.id).await.unwrap().unwrap();
    assert_eq!(got.id, doc.id);
    let list = store.list_alive(ws).await.unwrap();
    assert_eq!(list.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn rename_updates_title_and_icon() {
    let (store, ws, user) = setup().await;
    let sk = sort_key_between(None, None);
    let doc = store.create(ws, None, "Old", &sk, user).await.unwrap();
    let new = store.rename(ws, doc.id, user, "New", Some("📄")).await.unwrap();
    assert_eq!(new.title, "New");
    assert_eq!(new.icon.as_deref(), Some("📄"));
}

#[tokio::test(flavor = "multi_thread")]
async fn archive_hides_from_list_and_restore_brings_back() {
    let (store, ws, user) = setup().await;
    let sk = sort_key_between(None, None);
    let doc = store.create(ws, None, "X", &sk, user).await.unwrap();
    store.archive(ws, doc.id, user).await.unwrap();
    assert_eq!(store.list_alive(ws).await.unwrap().len(), 0);
    store.restore(ws, doc.id, user).await.unwrap();
    assert_eq!(store.list_alive(ws).await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn move_to_under_new_parent() {
    let (store, ws, user) = setup().await;
    let a = store.create(ws, None, "A", "m", user).await.unwrap();
    let b = store.create(ws, None, "B", "n", user).await.unwrap();
    // Move b under a.
    let moved = store.move_to(ws, b.id, user, Some(a.id), "m").await.unwrap();
    assert_eq!(moved.parent_id, Some(a.id));
    let kids = store.siblings(ws, Some(a.id)).await.unwrap();
    assert_eq!(kids.len(), 1);
    assert_eq!(kids[0].id, b.id);
}

#[tokio::test(flavor = "multi_thread")]
async fn rename_not_found() {
    let (store, ws, user) = setup().await;
    let err = store.rename(ws, Uuid::new_v4(), user, "X", None).await.unwrap_err();
    assert!(matches!(err, knot_storage::DocStoreError::NotFound));
}
```

- [ ] **Step 6: Verify**

```bash
cargo build -p knot-storage
cargo test -p knot-storage --test documents
cargo clippy -p knot-storage --all-targets --all-features -- -D warnings
```

Expected: 5 tests pass. Existing tests still green.

- [ ] **Step 7: Commit**

```bash
git add crates/knot-storage/
git commit -m "feat(knot-storage): real DocStore with audit + invalidation outbox writes"
```

---

## Task 5: GrantStore

**Files:**
- Create: `crates/knot-storage/src/grant_store.rs`
- Modify: `crates/knot-storage/src/lib.rs` — re-export
- Create: `crates/knot-storage/tests/grants.rs`

- [ ] **Step 1: GrantStore**

Create `crates/knot-storage/src/grant_store.rs`:

```rust
//! Per-document grants: explicit role for a principal on a doc, with
//! `inherit` controlling whether descendant docs see the grant too.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::WorkspaceRole;
use crate::audit;
use crate::invalidations;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub doc_id: Uuid,
    pub principal: String, // "user:<uuid>" or "group:<name>"
    pub role: WorkspaceRole,
    pub inherit: bool,
    pub granted_at: DateTime<Utc>,
    pub granted_by: Option<Uuid>,
}

#[derive(Debug, Error)]
pub enum GrantStoreError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("invalid role: {0}")]
    InvalidRole(String),
}

#[async_trait]
pub trait GrantStore: Send + Sync + 'static {
    async fn list(&self, doc_id: Uuid) -> Result<Vec<Grant>, GrantStoreError>;
    /// List grants attached to any document in the parent chain of `doc_id`,
    /// in walk order (deepest first). Only `inherit=true` grants from
    /// ancestors are returned; the doc's own grants are returned regardless.
    async fn list_inherited(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
    ) -> Result<Vec<Grant>, GrantStoreError>;
    async fn put(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        principal: &str,
        role: WorkspaceRole,
        inherit: bool,
        granted_by: Uuid,
    ) -> Result<(), GrantStoreError>;
    async fn delete(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        principal: &str,
        actor: Uuid,
    ) -> Result<(), GrantStoreError>;
}

#[derive(Clone)]
pub struct PgGrantStore {
    pool: PgPool,
}

impl PgGrantStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

type GrantRow = (Uuid, String, String, bool, DateTime<Utc>, Option<Uuid>);
fn from_row(r: GrantRow) -> Result<Grant, GrantStoreError> {
    let role = WorkspaceRole::parse(&r.2)
        .ok_or_else(|| GrantStoreError::InvalidRole(r.2.clone()))?;
    Ok(Grant {
        doc_id: r.0,
        principal: r.1,
        role,
        inherit: r.3,
        granted_at: r.4,
        granted_by: r.5,
    })
}

#[async_trait]
impl GrantStore for PgGrantStore {
    async fn list(&self, doc_id: Uuid) -> Result<Vec<Grant>, GrantStoreError> {
        let rows = sqlx::query_as::<_, GrantRow>(
            "SELECT doc_id, principal, role, inherit, granted_at, granted_by
             FROM document_grants WHERE doc_id = $1",
        )
        .bind(doc_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(from_row).collect()
    }

    async fn list_inherited(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
    ) -> Result<Vec<Grant>, GrantStoreError> {
        // Recursive CTE walks from doc_id up to root; selects grants on each.
        // For non-self levels, only inherit=true grants are returned.
        let rows = sqlx::query_as::<_, GrantRow>(
            "WITH RECURSIVE chain AS (
                 SELECT id, parent_id, 0 AS depth
                 FROM documents WHERE id = $2 AND workspace_id = $1
                 UNION ALL
                 SELECT d.id, d.parent_id, c.depth + 1
                 FROM documents d JOIN chain c ON d.id = c.parent_id
                 WHERE d.workspace_id = $1
             )
             SELECT g.doc_id, g.principal, g.role, g.inherit, g.granted_at, g.granted_by
             FROM document_grants g
             JOIN chain c ON g.doc_id = c.id
             WHERE c.depth = 0 OR g.inherit = true
             ORDER BY c.depth ASC",
        )
        .bind(workspace_id)
        .bind(doc_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(from_row).collect()
    }

    async fn put(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        principal: &str,
        role: WorkspaceRole,
        inherit: bool,
        granted_by: Uuid,
    ) -> Result<(), GrantStoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO document_grants (doc_id, principal, role, inherit, granted_by)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (doc_id, principal) DO UPDATE
             SET role = EXCLUDED.role, inherit = EXCLUDED.inherit,
                 granted_at = now(), granted_by = EXCLUDED.granted_by",
        )
        .bind(doc_id)
        .bind(principal)
        .bind(role.as_str())
        .bind(inherit)
        .bind(granted_by)
        .execute(&mut *tx)
        .await?;
        audit::record_in_tx(&mut tx, workspace_id, Some(granted_by), "doc.grant", "doc", doc_id).await?;
        invalidations::record_in_tx(&mut tx, workspace_id, doc_id, "grant-change").await?;
        tx.commit().await?;
        Ok(())
    }

    async fn delete(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        principal: &str,
        actor: Uuid,
    ) -> Result<(), GrantStoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM document_grants WHERE doc_id = $1 AND principal = $2")
            .bind(doc_id)
            .bind(principal)
            .execute(&mut *tx)
            .await?;
        audit::record_in_tx(&mut tx, workspace_id, Some(actor), "doc.grant.delete", "doc", doc_id).await?;
        invalidations::record_in_tx(&mut tx, workspace_id, doc_id, "grant-delete").await?;
        tx.commit().await?;
        Ok(())
    }
}
```

- [ ] **Step 2: lib.rs**

```rust
pub mod grant_store;
pub use grant_store::{Grant, GrantStore, GrantStoreError, PgGrantStore};
```

- [ ] **Step 3: Integration tests**

Create `crates/knot-storage/tests/grants.rs`:

```rust
use knot_storage::{
    DocStore, GrantStore, PgDocStore, PgGrantStore, PgUserStore, PgWorkspaceStore,
    UserStore, WorkspaceRole, WorkspaceStore,
};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
use uuid::Uuid;

async fn setup() -> (PgDocStore, PgGrantStore, Uuid, Uuid) {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    std::mem::forget(container);
    let ws = PgWorkspaceStore::new(pool.clone()).create("default", "W").await.unwrap();
    let users = PgUserStore::new(pool.clone());
    let u = users.create_local("a@x.test", "A", "$h$").await.unwrap();
    PgWorkspaceStore::new(pool.clone()).add_member(ws.id, u.id, WorkspaceRole::Owner).await.unwrap();
    (PgDocStore::new(pool.clone()), PgGrantStore::new(pool), ws.id, u.id)
}

#[tokio::test(flavor = "multi_thread")]
async fn put_list_delete() {
    let (docs, grants, ws, user) = setup().await;
    let d = docs.create(ws, None, "X", "m", user).await.unwrap();
    let principal = format!("user:{}", user);
    grants.put(ws, d.id, &principal, WorkspaceRole::Editor, true, user).await.unwrap();
    let l = grants.list(d.id).await.unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].role, WorkspaceRole::Editor);
    grants.delete(ws, d.id, &principal, user).await.unwrap();
    assert!(grants.list(d.id).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn put_updates_existing() {
    let (docs, grants, ws, user) = setup().await;
    let d = docs.create(ws, None, "X", "m", user).await.unwrap();
    let principal = format!("user:{}", user);
    grants.put(ws, d.id, &principal, WorkspaceRole::Viewer, true, user).await.unwrap();
    grants.put(ws, d.id, &principal, WorkspaceRole::Owner, false, user).await.unwrap();
    let l = grants.list(d.id).await.unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].role, WorkspaceRole::Owner);
    assert!(!l[0].inherit);
}

#[tokio::test(flavor = "multi_thread")]
async fn inherited_includes_ancestor_inherit_true() {
    let (docs, grants, ws, user) = setup().await;
    let root = docs.create(ws, None, "Root", "m", user).await.unwrap();
    let child = docs.create(ws, Some(root.id), "Child", "m", user).await.unwrap();
    let principal = format!("user:{}", user);
    grants.put(ws, root.id, &principal, WorkspaceRole::Editor, true, user).await.unwrap();
    let inh = grants.list_inherited(ws, child.id).await.unwrap();
    assert_eq!(inh.len(), 1);
    assert_eq!(inh[0].role, WorkspaceRole::Editor);
}

#[tokio::test(flavor = "multi_thread")]
async fn inherited_skips_ancestor_inherit_false() {
    let (docs, grants, ws, user) = setup().await;
    let root = docs.create(ws, None, "Root", "m", user).await.unwrap();
    let child = docs.create(ws, Some(root.id), "Child", "m", user).await.unwrap();
    let principal = format!("user:{}", user);
    grants.put(ws, root.id, &principal, WorkspaceRole::Editor, false, user).await.unwrap();
    let inh = grants.list_inherited(ws, child.id).await.unwrap();
    assert!(inh.is_empty(), "ancestor inherit=false should not propagate");
}
```

- [ ] **Step 4: Verify + commit**

```bash
cargo test -p knot-storage --test grants
cargo clippy -p knot-storage --all-targets --all-features -- -D warnings
git add crates/knot-storage/
git commit -m "feat(knot-storage): GrantStore with inheritance walk"
```

---

## Task 6: ACL resolver (knot-docs crate)

**Files:**
- Modify root `Cargo.toml`: add `crates/knot-docs` member; add `moka = { version = "0.12", features = ["future"] }`
- Create: `crates/knot-docs/Cargo.toml`
- Create: `crates/knot-docs/src/lib.rs`
- Create: `crates/knot-docs/src/acl.rs`
- Modify: `crates/knot-server/Cargo.toml` — `knot-docs = { path = "../knot-docs" }`

- [ ] **Step 1: Workspace + crate manifest**

Edit root `Cargo.toml` — add `"crates/knot-docs",` to members and add to `[workspace.dependencies]`:

```toml
moka = { version = "0.12", features = ["future"] }
```

Create `crates/knot-docs/Cargo.toml`:

```toml
[package]
name = "knot-docs"
version = "0.0.0"
edition.workspace = true
license.workspace = true
publish = false

[dependencies]
knot-storage = { path = "../knot-storage" }
async-trait.workspace = true
moka.workspace = true
sqlx.workspace = true
thiserror.workspace = true
tokio.workspace = true
tracing.workspace = true
uuid.workspace = true

[dev-dependencies]
testcontainers.workspace = true
testcontainers-modules.workspace = true
chrono.workspace = true
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Create `crates/knot-docs/src/lib.rs`:

```rust
//! ACL resolver + cache + invalidation listener for knot.

pub mod acl;
pub mod cache;
pub mod listener;

pub use acl::{EffectiveRole, resolve};
pub use cache::AclCache;
pub use listener::spawn_listener;
```

- [ ] **Step 2: acl.rs**

Create `crates/knot-docs/src/acl.rs`:

```rust
//! Resolve the effective role for (doc_id, user_id).
//!
//! Algorithm:
//! 1. Look up the user's workspace role. Owner > Editor > Viewer >
//!    Non-member.
//! 2. Walk grants from the doc up to root (via GrantStore::list_inherited).
//!    The first grant matching `user:<user_id>` wins, but only if it
//!    upgrades the current effective role (we always take the max of
//!    explicit-grant role and workspace role).
//! 3. Return the highest role found, or None if the user isn't a member.

use knot_storage::{GrantStore, GrantStoreError, WorkspaceRole, WorkspaceStore, WorkspaceStoreError};
use thiserror::Error;
use uuid::Uuid;

pub type EffectiveRole = WorkspaceRole;

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error("workspace: {0}")]
    Workspace(#[from] WorkspaceStoreError),
    #[error("grants: {0}")]
    Grants(#[from] GrantStoreError),
}

fn rank(r: WorkspaceRole) -> u8 {
    match r {
        WorkspaceRole::Owner => 3,
        WorkspaceRole::Editor => 2,
        WorkspaceRole::Viewer => 1,
    }
}

fn max(a: WorkspaceRole, b: WorkspaceRole) -> WorkspaceRole {
    if rank(a) >= rank(b) { a } else { b }
}

pub async fn resolve(
    workspaces: &dyn WorkspaceStore,
    grants: &dyn GrantStore,
    workspace_id: Uuid,
    doc_id: Uuid,
    user_id: Uuid,
) -> Result<Option<EffectiveRole>, ResolveError> {
    let workspace_role = workspaces.get_member_role(workspace_id, user_id).await?;
    let principal = format!("user:{user_id}");
    let inherited = grants.list_inherited(workspace_id, doc_id).await?;
    let grant_role = inherited
        .into_iter()
        .filter(|g| g.principal == principal)
        .map(|g| g.role)
        .reduce(max);
    Ok(match (workspace_role, grant_role) {
        (None, None) => None,
        (Some(w), None) => Some(w),
        (None, Some(g)) => Some(g),
        (Some(w), Some(g)) => Some(max(w, g)),
    })
}
```

- [ ] **Step 3: Unit tests**

Append to `acl.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use knot_storage::{
        DocStore, PgDocStore, PgGrantStore, PgUserStore, PgWorkspaceStore, UserStore,
        WorkspaceStore as WSTrait,
    };
    use sqlx::postgres::PgPoolOptions;
    use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

    async fn ctx() -> (PgWorkspaceStore, PgGrantStore, PgDocStore, Uuid, Uuid) {
        let c = Postgres::default().start().await.unwrap();
        let port = c.get_host_port_ipv4(5432).await.unwrap();
        let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
        let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        std::mem::forget(c);

        let ws_s = PgWorkspaceStore::new(pool.clone());
        let us = PgUserStore::new(pool.clone());
        let ds = PgDocStore::new(pool.clone());
        let gs = PgGrantStore::new(pool);
        let w = ws_s.create("default", "W").await.unwrap();
        let u = us.create_local("a@x.test", "A", "$h$").await.unwrap();
        ws_s.add_member(w.id, u.id, WorkspaceRole::Viewer).await.unwrap();
        (ws_s, gs, ds, w.id, u.id)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn workspace_role_used_when_no_grant() {
        let (ws_s, gs, ds, ws, user) = ctx().await;
        let d = ds.create(ws, None, "X", "m", user).await.unwrap();
        let r = resolve(&ws_s, &gs, ws, d.id, user).await.unwrap();
        assert_eq!(r, Some(WorkspaceRole::Viewer));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn explicit_grant_upgrades_role() {
        let (ws_s, gs, ds, ws, user) = ctx().await;
        let d = ds.create(ws, None, "X", "m", user).await.unwrap();
        gs.put(ws, d.id, &format!("user:{user}"), WorkspaceRole::Owner, true, user).await.unwrap();
        let r = resolve(&ws_s, &gs, ws, d.id, user).await.unwrap();
        assert_eq!(r, Some(WorkspaceRole::Owner));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn ancestor_inherit_propagates() {
        let (ws_s, gs, ds, ws, user) = ctx().await;
        let root = ds.create(ws, None, "R", "m", user).await.unwrap();
        let child = ds.create(ws, Some(root.id), "C", "m", user).await.unwrap();
        gs.put(ws, root.id, &format!("user:{user}"), WorkspaceRole::Editor, true, user).await.unwrap();
        let r = resolve(&ws_s, &gs, ws, child.id, user).await.unwrap();
        assert_eq!(r, Some(WorkspaceRole::Editor));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn non_member_with_no_grant_is_none() {
        let (ws_s, gs, ds, ws, owner) = ctx().await;
        let d = ds.create(ws, None, "X", "m", owner).await.unwrap();
        let other = Uuid::new_v4();
        let r = resolve(&ws_s, &gs, ws, d.id, other).await.unwrap();
        assert_eq!(r, None);
    }
}
```

- [ ] **Step 4: Verify + commit**

```bash
cargo build -p knot-docs
cargo test -p knot-docs --lib
cargo clippy -p knot-docs --all-targets --all-features -- -D warnings
git add Cargo.toml Cargo.lock crates/knot-docs/
git commit -m "feat(knot-docs): ACL resolver (workspace role + grant inheritance)"
```

---

## Task 7: AclCache (moka)

**Files:**
- Create: `crates/knot-docs/src/cache.rs`

- [ ] **Step 1: Cache wrapper**

Create `crates/knot-docs/src/cache.rs`:

```rust
//! moka-backed cache around `acl::resolve`.
//!
//! TTL: 60 s (spec §7.5). Capacity: 100k entries (per process; well
//! within memory budget for v0.1).
//!
//! Invalidations: `evict_doc(doc_id)` is called by the listener task
//! (see `listener.rs`) when an `acl_invalidate` NOTIFY arrives. For a
//! grant-change on a doc that has descendants with inherit=true, the
//! listener walks the subtree and evicts each (doc_id, *) key — that
//! pass lives in the listener, not here.

use std::sync::Arc;
use std::time::Duration;

use knot_storage::{GrantStore, WorkspaceRole, WorkspaceStore};
use moka::future::Cache;
use uuid::Uuid;

use crate::acl::{ResolveError, resolve};

#[derive(Clone)]
pub struct AclCache {
    inner: Cache<(Uuid, Uuid), Option<WorkspaceRole>>,
    workspaces: Arc<dyn WorkspaceStore>,
    grants: Arc<dyn GrantStore>,
}

impl AclCache {
    pub fn new(workspaces: Arc<dyn WorkspaceStore>, grants: Arc<dyn GrantStore>) -> Self {
        let inner = Cache::builder()
            .max_capacity(100_000)
            .time_to_live(Duration::from_secs(60))
            .build();
        Self { inner, workspaces, grants }
    }

    pub async fn effective_role(
        &self,
        workspace_id: Uuid,
        doc_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<WorkspaceRole>, ResolveError> {
        let key = (doc_id, user_id);
        if let Some(v) = self.inner.get(&key).await {
            return Ok(v);
        }
        let v = resolve(
            self.workspaces.as_ref(),
            self.grants.as_ref(),
            workspace_id,
            doc_id,
            user_id,
        )
        .await?;
        self.inner.insert(key, v).await;
        Ok(v)
    }

    pub async fn evict_doc(&self, doc_id: Uuid) {
        // moka has no "iter+remove by partial key" — invalidate_entries_if
        // gives us a filter. Synchronous iteration over the keys would be
        // ~100ms worst-case at 100k; the predicate runs lazily.
        self.inner
            .invalidate_entries_if(move |k, _| k.0 == doc_id)
            .ok();
    }

    pub async fn evict_all(&self) {
        self.inner.invalidate_all();
    }
}
```

> Note: `Cache::invalidate_entries_if` requires the `future` feature on moka (already enabled).

- [ ] **Step 2: Verify**

```bash
cargo build -p knot-docs
cargo clippy -p knot-docs --all-targets --all-features -- -D warnings
```

- [ ] **Step 3: Commit**

```bash
git add crates/knot-docs/
git commit -m "feat(knot-docs): AclCache (moka, 60s TTL) with evict_doc + evict_all"
```

---

## Task 8: ACL invalidations outbox writers

This task was actually performed in T4 (DocStore writes) and T5 (GrantStore writes). T8 verifies the invalidations are written correctly + adds a dedicated test.

**Files:**
- Create: `crates/knot-storage/tests/invalidations.rs`

- [ ] **Step 1: Test that mutations write invalidation rows**

Create `crates/knot-storage/tests/invalidations.rs`:

```rust
use knot_storage::{
    DocStore, GrantStore, PgDocStore, PgGrantStore, PgUserStore, PgWorkspaceStore,
    UserStore, WorkspaceRole, WorkspaceStore,
};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

async fn count_invalidations(pool: &sqlx::PgPool, doc_id: uuid::Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM acl_invalidations WHERE doc_id = $1",
    )
    .bind(doc_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn doc_create_move_grant_each_write_invalidation() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    std::mem::forget(container);

    let ws = PgWorkspaceStore::new(pool.clone()).create("default", "W").await.unwrap();
    let u = PgUserStore::new(pool.clone()).create_local("a@x.test", "A", "$h$").await.unwrap();
    PgWorkspaceStore::new(pool.clone()).add_member(ws.id, u.id, WorkspaceRole::Owner).await.unwrap();

    let docs = PgDocStore::new(pool.clone());
    let grants = PgGrantStore::new(pool.clone());

    let d = docs.create(ws.id, None, "X", "m", u.id).await.unwrap();
    assert_eq!(count_invalidations(&pool, d.id).await, 1, "create");

    docs.move_to(ws.id, d.id, u.id, None, "n").await.unwrap();
    assert_eq!(count_invalidations(&pool, d.id).await, 2, "+ move");

    grants.put(ws.id, d.id, &format!("user:{}", u.id), WorkspaceRole::Editor, true, u.id).await.unwrap();
    assert_eq!(count_invalidations(&pool, d.id).await, 3, "+ grant put");

    grants.delete(ws.id, d.id, &format!("user:{}", u.id), u.id).await.unwrap();
    assert_eq!(count_invalidations(&pool, d.id).await, 4, "+ grant delete");
}
```

- [ ] **Step 2: Verify + commit**

```bash
cargo test -p knot-storage --test invalidations
git add crates/knot-storage/tests/invalidations.rs
git commit -m "test(knot-storage): mutations write to acl_invalidations outbox"
```

---

## Task 9: ACL NOTIFY listener task

**Files:**
- Create: `crates/knot-docs/src/listener.rs`

- [ ] **Step 1: Listener**

Create `crates/knot-docs/src/listener.rs`:

```rust
//! Postgres LISTEN consumer.
//!
//! Subscribes to channel `acl_invalidate` (emitted by writers in
//! knot-storage::invalidations). Payload is a doc_id text. On each
//! notification, evicts cache entries keyed on that doc.
//!
//! Best-effort: reconnect with backoff on disconnect.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::AclCache;

const CHANNEL: &str = "acl_invalidate";

pub fn spawn_listener(pool: PgPool, cache: Arc<AclCache>) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match run_once(&pool, &cache).await {
                Ok(()) => {
                    tracing::warn!("acl listener exited cleanly; reconnecting");
                }
                Err(e) => {
                    tracing::warn!(error=?e, "acl listener error; reconnecting in 5s");
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
}

async fn run_once(pool: &PgPool, cache: &AclCache) -> Result<(), sqlx::Error> {
    let mut listener = PgListener::connect_with(pool).await?;
    listener.listen(CHANNEL).await?;
    tracing::info!("acl listener subscribed to {CHANNEL}");
    loop {
        let n = listener.recv().await?;
        let payload = n.payload();
        match payload.parse::<Uuid>() {
            Ok(doc_id) => {
                tracing::debug!(%doc_id, "acl evict");
                cache.evict_doc(doc_id).await;
                // GC the outbox row.
                let _ = sqlx::query(
                    "DELETE FROM acl_invalidations WHERE doc_id = $1 AND created_at <= now()",
                )
                .bind(doc_id)
                .execute(pool)
                .await;
            }
            Err(_) => {
                tracing::warn!(payload, "malformed acl_invalidate payload; evicting all");
                cache.evict_all().await;
            }
        }
    }
}
```

- [ ] **Step 2: Listener test**

Create `crates/knot-docs/tests/listener_integration.rs`:

```rust
use std::sync::Arc;
use std::time::Duration;

use knot_docs::{AclCache, spawn_listener};
use knot_storage::{
    DocStore, GrantStore, PgDocStore, PgGrantStore, PgUserStore, PgWorkspaceStore,
    UserStore, WorkspaceRole, WorkspaceStore,
};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};

#[tokio::test(flavor = "multi_thread")]
async fn grant_change_evicts_cache_entry() {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().max_connections(8).connect(&url).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    std::mem::forget(container);

    let ws_s = PgWorkspaceStore::new(pool.clone());
    let us = PgUserStore::new(pool.clone());
    let ds = PgDocStore::new(pool.clone());
    let gs = PgGrantStore::new(pool.clone());

    let ws = ws_s.create("default", "W").await.unwrap();
    let u = us.create_local("a@x.test", "A", "$h$").await.unwrap();
    ws_s.add_member(ws.id, u.id, WorkspaceRole::Viewer).await.unwrap();
    let d = ds.create(ws.id, None, "X", "m", u.id).await.unwrap();

    let cache = Arc::new(AclCache::new(Arc::new(ws_s.clone()), Arc::new(gs.clone())));
    let _handle = spawn_listener(pool.clone(), cache.clone());
    tokio::time::sleep(Duration::from_millis(200)).await; // let listener subscribe

    // Prime cache.
    let r1 = cache.effective_role(ws.id, d.id, u.id).await.unwrap();
    assert_eq!(r1, Some(WorkspaceRole::Viewer));

    // Grant upgrade.
    gs.put(ws.id, d.id, &format!("user:{}", u.id), WorkspaceRole::Owner, true, u.id).await.unwrap();
    // Wait for NOTIFY → evict.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Re-resolve — should see upgraded role now.
    let r2 = cache.effective_role(ws.id, d.id, u.id).await.unwrap();
    assert_eq!(r2, Some(WorkspaceRole::Owner));
}
```

- [ ] **Step 3: Verify + commit**

```bash
cargo build -p knot-docs
cargo test -p knot-docs --test listener_integration
git add crates/knot-docs/
git commit -m "feat(knot-docs): PgListener task evicts AclCache on acl_invalidate"
```

---

## Task 10: RequireDocRole middleware

**Files:**
- Modify: `crates/knot-server/Cargo.toml` — `knot-docs = { path = "../knot-docs" }`
- Create: `crates/knot-server/src/auth/require_doc_role.rs`
- Modify: `crates/knot-server/src/auth/mod.rs` — re-export
- Modify: `crates/knot-server/src/lib.rs` — AppState gets `Option<Arc<AclCache>>`; populated in `with_pool`

- [ ] **Step 1: Dep + AppState**

Add to `crates/knot-server/Cargo.toml`:

```toml
knot-docs = { path = "../knot-docs" }
```

Edit `crates/knot-server/src/lib.rs`:

Add import:

```rust
use knot_docs::AclCache;
use knot_storage::PgGrantStore;
```

Add to AppState:

```rust
    pub grants: Option<Arc<dyn knot_storage::GrantStore>>,
    pub docs: Option<Arc<dyn knot_storage::DocStore>>,
    pub acl: Option<Arc<AclCache>>,
```

In `in_memory()`:

```rust
            grants: None,
            docs: None,
            acl: None,
```

In `with_pool`, after creating the existing stores, add:

```rust
        let docs: Arc<dyn knot_storage::DocStore> = Arc::new(knot_storage::PgDocStore::new(pool.clone()));
        let grants: Arc<dyn knot_storage::GrantStore> = Arc::new(PgGrantStore::new(pool.clone()));
        let acl = Arc::new(AclCache::new(workspaces.clone(), grants.clone()));
```

And set:

```rust
            grants: Some(grants),
            docs: Some(docs),
            acl: Some(acl),
```

- [ ] **Step 2: Middleware**

Create `crates/knot-server/src/auth/require_doc_role.rs`:

```rust
//! Route-scoped middleware: parses doc_id from path, resolves the caller's
//! effective role via the AclCache, and inserts the role into request
//! extensions for the downstream handler.
//!
//! A handler that requires this middleware should extract
//! `EffectiveDocRole` and consult `.minimum(WorkspaceRole::Editor)` etc.
//! The middleware itself only enforces "is at least Viewer" (=non-None).

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use knot_storage::WorkspaceRole;
use uuid::Uuid;

use super::context::AuthContext;
use crate::AppState;
use crate::http_error::json_err;

#[derive(Debug, Clone, Copy)]
pub struct EffectiveDocRole(pub WorkspaceRole);

pub async fn require_doc_role_mw(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(acl) = state.acl.clone() else {
        return json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "");
    };
    let role = match acl.effective_role(ctx.workspace_id, doc_id, ctx.user_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return json_err(StatusCode::FORBIDDEN, "acl.no_grant", ""),
        Err(e) => {
            tracing::error!(error=?e, "acl resolve");
            return json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "");
        }
    };
    req.extensions_mut().insert(EffectiveDocRole(role));
    next.run(req).await
}
```

- [ ] **Step 3: Re-export**

Edit `crates/knot-server/src/auth/mod.rs`:

```rust
pub mod require_doc_role;
pub use require_doc_role::{EffectiveDocRole, require_doc_role_mw};
```

- [ ] **Step 4: Verify + commit**

```bash
cargo build -p knot-server
cargo clippy -p knot-server --all-targets --all-features -- -D warnings
git add crates/knot-server/ Cargo.lock
git commit -m "feat(knot-server): RequireDocRole middleware + AclCache wired into AppState"
```

---

## Task 11: /api router + CSRF + RequireSession layering

**Files:**
- Modify: `crates/knot-server/src/routes/api/mod.rs`
- Modify: `crates/knot-server/src/lib.rs` — layer csrf + require_session on the /api subtree
- Modify: `crates/knot-server/src/main.rs` — spawn the ACL listener

- [ ] **Step 1: api router layers auth + csrf**

Edit `crates/knot-server/src/routes/api/mod.rs`:

```rust
//! `/api/*` routes. Csrf + RequireSession layered here.

use axum::{Router, middleware};

use crate::AppState;
use crate::auth::{csrf_mw, require_session_mw};

pub mod workspace;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(workspace::router())
        .layer(middleware::from_fn(csrf_mw))
        .layer(middleware::from_fn(require_session_mw))
}
```

The outer layer (`require_session_mw`) runs first, ensuring AuthContext is present; csrf_mw runs second and is now a no-op for safe methods and a real check for unsafe methods.

- [ ] **Step 2: Spawn ACL listener at startup**

Edit `crates/knot-server/src/main.rs`. After AppState construction, before bind:

```rust
    if let (Some(pool), Some(acl)) = (state.pool.clone(), state.acl.clone()) {
        knot_docs::spawn_listener(pool, acl);
    }
```

- [ ] **Step 3: Verify**

```bash
cargo build -p knot-server
cargo clippy -p knot-server --all-targets --all-features -- -D warnings
cargo test -p knot-server
```

Expected: existing tests still pass. The middleware layers wrap the api subtree, leaving health/auth/collab unaffected.

- [ ] **Step 4: Commit**

```bash
git add crates/knot-server/
git commit -m "feat(knot-server): /api/* gets CSRF + RequireSession; spawn ACL listener"
```

---

## Task 12: GET /api/docs + POST /api/docs + GET /api/docs/:id

**Files:**
- Create: `crates/knot-server/src/routes/api/docs.rs`
- Modify: `crates/knot-server/src/routes/api/mod.rs` — mount docs

- [ ] **Step 1: docs.rs**

Create `crates/knot-server/src/routes/api/docs.rs`:

```rust
//! Documents API:
//! - GET    /api/docs            flat list (alive only)
//! - POST   /api/docs            body: {title?, parent_id?, after_id?}
//! - GET    /api/docs/:id        metadata + effective_role
//! - PATCH  /api/docs/:id        body: {title?, icon?}
//! - POST   /api/docs/:id/move   body: {parent_id?, after_id?, before_id?}
//! - DELETE /api/docs/:id        soft-delete
//! - POST   /api/docs/:id/restore

use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use knot_storage::{Document, WorkspaceRole, sort_key_between};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::auth::{AuthContext, EffectiveDocRole, require_doc_role_mw};
use crate::http_error::json_err;

#[derive(Serialize)]
struct DocResponse {
    id: String,
    workspace_id: String,
    parent_id: Option<String>,
    title: String,
    sort_key: String,
    icon: Option<String>,
    created_by: String,
    archived: bool,
}

fn to_response(d: &Document) -> DocResponse {
    DocResponse {
        id: d.id.to_string(),
        workspace_id: d.workspace_id.to_string(),
        parent_id: d.parent_id.map(|u| u.to_string()),
        title: d.title.clone(),
        sort_key: d.sort_key.clone(),
        icon: d.icon.clone(),
        created_by: d.created_by.to_string(),
        archived: d.archived_at.is_some(),
    }
}

pub fn router() -> Router<AppState> {
    let with_role = Router::new()
        .route("/api/docs/:id", get(get_one))
        .layer(middleware::from_fn_with_state(
            (), // placeholder — see note below
            require_doc_role_mw,
        ));
    let without_role = Router::new()
        .route("/api/docs", get(list).post(create));

    without_role.merge(with_role)
}
```

> **Note for implementer:** `from_fn_with_state` requires the state type. Because `require_doc_role_mw` declares `State<AppState>` directly, we use `from_fn` (not `from_fn_with_state`) and the State extractor pulls from the merged router state. Adjust the imports accordingly. The pseudo `(),` above is a placeholder — replace with `from_fn(require_doc_role_mw)` once you wire it via the router that already has State<AppState>.

Replace the `pub fn router()` body with the simpler, correct form:

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/docs", get(list).post(create))
        .route("/api/docs/:id", get(get_one).patch(rename).delete(archive))
        .route("/api/docs/:id/move", post(move_doc))
        .route("/api/docs/:id/restore", post(restore))
        .layer(middleware::from_fn(require_doc_role_passthrough))
}
```

Where `require_doc_role_passthrough` is a small wrapper that conditionally invokes the doc-role middleware ONLY when the path matches `/api/docs/:id/*`. Actually the simpler shape is:

```rust
pub fn router() -> Router<AppState> {
    let with_role: Router<AppState> = Router::new()
        .route("/api/docs/:id", get(get_one).patch(rename).delete(archive))
        .route("/api/docs/:id/move", post(move_doc))
        .route("/api/docs/:id/restore", post(restore))
        .layer(middleware::from_fn_with_state(
            std::marker::PhantomData::<AppState>,
            // — see implementer note —
            crate::auth::require_doc_role_mw,
        ));
    let without_role: Router<AppState> = Router::new()
        .route("/api/docs", get(list).post(create));
    without_role.merge(with_role)
}
```

> **Implementer note (axum 0.7 middleware):** `from_fn` is the correct API when the middleware's `State<AppState>` is provided by the surrounding router via `with_state()` (which happens in lib.rs when `router_with_state` is invoked). Use:
>
> ```rust
> .layer(middleware::from_fn(crate::auth::require_doc_role_mw))
> ```
>
> The `State<AppState>` extractor inside `require_doc_role_mw` will resolve from the parent router's state. No `from_fn_with_state` is needed.

Define the handlers below:

```rust
async fn list(State(state): State<AppState>, req: Request) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(docs) = state.docs.clone() else { return internal() };
    match docs.list_alive(ctx.workspace_id).await {
        Ok(list) => Json(list.iter().map(to_response).collect::<Vec<_>>()).into_response(),
        Err(e) => {
            tracing::error!(error=?e, "list");
            internal()
        }
    }
}

#[derive(Deserialize)]
struct CreateRequest {
    title: Option<String>,
    parent_id: Option<Uuid>,
    after_id: Option<Uuid>,
}

async fn create(State(state): State<AppState>, req: Request) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    if ctx.role == WorkspaceRole::Viewer {
        return json_err(StatusCode::FORBIDDEN, "acl.editor_required", "");
    }
    let Ok(body) = read_json::<CreateRequest>(req).await else {
        return json_err(StatusCode::BAD_REQUEST, "bad_request", "");
    };
    let Some(docs) = state.docs.clone() else { return internal() };
    let title = body.title.unwrap_or_else(|| "Untitled".into());

    // Compute sort_key between after_id and after_id's next sibling.
    let siblings = match docs.siblings(ctx.workspace_id, body.parent_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error=?e, "siblings");
            return internal();
        }
    };
    let (a, b) = match body.after_id {
        None => (
            None,
            siblings.first().map(|d| d.sort_key.as_str()),
        ),
        Some(aid) => {
            let i = siblings.iter().position(|d| d.id == aid);
            match i {
                Some(i) => (
                    Some(siblings[i].sort_key.as_str()),
                    siblings.get(i + 1).map(|d| d.sort_key.as_str()),
                ),
                None => (
                    siblings.last().map(|d| d.sort_key.as_str()),
                    None,
                ),
            }
        }
    };
    let sk = sort_key_between(a, b);

    match docs.create(ctx.workspace_id, body.parent_id, &title, &sk, ctx.user_id).await {
        Ok(d) => (StatusCode::CREATED, Json(to_response(&d))).into_response(),
        Err(e) => {
            tracing::error!(error=?e, "create");
            internal()
        }
    }
}

async fn get_one(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    req: Request,
) -> Response {
    let Some(_ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(role) = req.extensions().get::<EffectiveDocRole>().copied() else {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    };
    let Some(docs) = state.docs.clone() else { return internal() };
    let doc = match docs.get(doc_id).await {
        Ok(Some(d)) => d,
        Ok(None) => return json_err(StatusCode::NOT_FOUND, "doc.not_found", ""),
        Err(e) => {
            tracing::error!(error=?e, "get");
            return internal();
        }
    };
    #[derive(Serialize)]
    struct GetResponse {
        #[serde(flatten)]
        doc: DocResponse,
        effective_role: String,
    }
    Json(GetResponse {
        doc: to_response(&doc),
        effective_role: role.0.as_str().into(),
    })
    .into_response()
}

// rename / move / archive / restore stubs (T13, T14 fill them in).
async fn rename(_p: Path<Uuid>) -> Response { json_err(StatusCode::NOT_IMPLEMENTED, "not_implemented", "") }
async fn move_doc(_p: Path<Uuid>) -> Response { json_err(StatusCode::NOT_IMPLEMENTED, "not_implemented", "") }
async fn archive(_p: Path<Uuid>) -> Response { json_err(StatusCode::NOT_IMPLEMENTED, "not_implemented", "") }
async fn restore(_p: Path<Uuid>) -> Response { json_err(StatusCode::NOT_IMPLEMENTED, "not_implemented", "") }

async fn read_json<T: serde::de::DeserializeOwned>(req: Request) -> Result<T, ()> {
    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024).await.map_err(|_| ())?;
    serde_json::from_slice(&bytes).map_err(|_| ())
}

fn internal() -> Response {
    json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "")
}
```

- [ ] **Step 2: Mount in /api/mod.rs**

Edit `crates/knot-server/src/routes/api/mod.rs`:

```rust
pub mod docs;
pub mod workspace;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(workspace::router())
        .merge(docs::router())
        .layer(middleware::from_fn(csrf_mw))
        .layer(middleware::from_fn(require_session_mw))
}
```

- [ ] **Step 3: Verify + commit**

```bash
cargo build -p knot-server
cargo clippy -p knot-server --all-targets --all-features -- -D warnings
git add crates/knot-server/
git commit -m "feat(knot-server): GET/POST /api/docs + GET /api/docs/:id with effective_role"
```

---

## Task 13: PATCH /api/docs/:id + POST /api/docs/:id/move

**Files:**
- Modify: `crates/knot-server/src/routes/api/docs.rs`

- [ ] **Step 1: Implement rename**

Replace the `rename` stub with:

```rust
#[derive(Deserialize)]
struct PatchRequest {
    title: Option<String>,
    icon: Option<String>,
}

async fn rename(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    req: Request,
) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(role) = req.extensions().get::<EffectiveDocRole>().copied() else {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    };
    if role.0 == WorkspaceRole::Viewer {
        return json_err(StatusCode::FORBIDDEN, "acl.editor_required", "");
    }
    let Ok(body) = read_json::<PatchRequest>(req).await else {
        return json_err(StatusCode::BAD_REQUEST, "bad_request", "");
    };
    let Some(docs) = state.docs.clone() else { return internal() };
    let cur = match docs.get(doc_id).await {
        Ok(Some(d)) => d,
        Ok(None) => return json_err(StatusCode::NOT_FOUND, "doc.not_found", ""),
        Err(e) => {
            tracing::error!(error=?e, "rename get");
            return internal();
        }
    };
    let title = body.title.as_deref().unwrap_or(&cur.title);
    match docs.rename(ctx.workspace_id, doc_id, ctx.user_id, title, body.icon.as_deref()).await {
        Ok(d) => Json(to_response(&d)).into_response(),
        Err(knot_storage::DocStoreError::NotFound) => json_err(StatusCode::NOT_FOUND, "doc.not_found", ""),
        Err(e) => {
            tracing::error!(error=?e, "rename");
            internal()
        }
    }
}
```

- [ ] **Step 2: Implement move**

Replace the `move_doc` stub with:

```rust
#[derive(Deserialize)]
struct MoveRequest {
    parent_id: Option<Uuid>,
    after_id: Option<Uuid>,
    before_id: Option<Uuid>,
}

async fn move_doc(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    req: Request,
) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(role) = req.extensions().get::<EffectiveDocRole>().copied() else {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    };
    if role.0 == WorkspaceRole::Viewer {
        return json_err(StatusCode::FORBIDDEN, "acl.editor_required", "");
    }
    let Ok(body) = read_json::<MoveRequest>(req).await else {
        return json_err(StatusCode::BAD_REQUEST, "bad_request", "");
    };
    let Some(docs) = state.docs.clone() else { return internal() };

    // Determine target parent: explicit body.parent_id, or fall back to the
    // doc's current parent.
    let cur = match docs.get(doc_id).await {
        Ok(Some(d)) => d,
        Ok(None) => return json_err(StatusCode::NOT_FOUND, "doc.not_found", ""),
        Err(e) => {
            tracing::error!(error=?e, "move get");
            return internal();
        }
    };
    let new_parent = body.parent_id.or(cur.parent_id);

    let siblings = match docs.siblings(ctx.workspace_id, new_parent).await {
        Ok(s) => s.into_iter().filter(|d| d.id != doc_id).collect::<Vec<_>>(),
        Err(e) => {
            tracing::error!(error=?e, "move siblings");
            return internal();
        }
    };
    let (a, b) = match (body.after_id, body.before_id) {
        (Some(aid), _) => {
            let i = siblings.iter().position(|d| d.id == aid);
            (
                i.map(|i| siblings[i].sort_key.as_str()),
                i.and_then(|i| siblings.get(i + 1)).map(|d| d.sort_key.as_str()),
            )
        }
        (_, Some(bid)) => {
            let i = siblings.iter().position(|d| d.id == bid);
            (
                i.and_then(|i| i.checked_sub(1)).and_then(|i| siblings.get(i)).map(|d| d.sort_key.as_str()),
                i.map(|i| siblings[i].sort_key.as_str()),
            )
        }
        (None, None) => (siblings.last().map(|d| d.sort_key.as_str()), None),
    };
    let sk = sort_key_between(a, b);

    match docs.move_to(ctx.workspace_id, doc_id, ctx.user_id, new_parent, &sk).await {
        Ok(d) => Json(to_response(&d)).into_response(),
        Err(knot_storage::DocStoreError::NotFound) => json_err(StatusCode::NOT_FOUND, "doc.not_found", ""),
        Err(knot_storage::DocStoreError::Conflict) => json_err(StatusCode::CONFLICT, "doc.sort_key_conflict", ""),
        Err(e) => {
            tracing::error!(error=?e, "move");
            internal()
        }
    }
}
```

- [ ] **Step 3: Verify + commit**

```bash
cargo build -p knot-server
cargo clippy -p knot-server --all-targets --all-features -- -D warnings
git add crates/knot-server/
git commit -m "feat(knot-server): PATCH /api/docs/:id + POST /api/docs/:id/move"
```

---

## Task 14: DELETE /api/docs/:id + POST /api/docs/:id/restore

**Files:**
- Modify: `crates/knot-server/src/routes/api/docs.rs`

- [ ] **Step 1: Implement archive + restore**

Replace the `archive` and `restore` stubs:

```rust
async fn archive(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    req: Request,
) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(role) = req.extensions().get::<EffectiveDocRole>().copied() else {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    };
    if role.0 != WorkspaceRole::Owner {
        return json_err(StatusCode::FORBIDDEN, "acl.owner_required", "");
    }
    let Some(docs) = state.docs.clone() else { return internal() };
    match docs.archive(ctx.workspace_id, doc_id, ctx.user_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(knot_storage::DocStoreError::NotFound) => json_err(StatusCode::NOT_FOUND, "doc.not_found", ""),
        Err(e) => {
            tracing::error!(error=?e, "archive");
            internal()
        }
    }
}

async fn restore(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    req: Request,
) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(role) = req.extensions().get::<EffectiveDocRole>().copied() else {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    };
    if role.0 != WorkspaceRole::Owner {
        return json_err(StatusCode::FORBIDDEN, "acl.owner_required", "");
    }
    let Some(docs) = state.docs.clone() else { return internal() };
    match docs.restore(ctx.workspace_id, doc_id, ctx.user_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(knot_storage::DocStoreError::NotFound) => json_err(StatusCode::NOT_FOUND, "doc.not_found", ""),
        Err(e) => {
            tracing::error!(error=?e, "restore");
            internal()
        }
    }
}
```

> **Note on archived docs + RequireDocRole:** an archived doc still has a row, and `RequireDocRole` resolves successfully against its row. Restore is therefore reachable. Archive is reachable on live docs.

- [ ] **Step 2: Integration test for docs handlers**

Create `crates/knot-server/tests/docs_integration.rs`:

```rust
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use knot_auth::{Hasher, Throttle};
use knot_server::{AppState, router_with_state};
use knot_storage::{WorkspaceRole, WorkspaceStore};
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::{postgres::Postgres, testcontainers::runners::AsyncRunner};
use tower::ServiceExt;

async fn login_state(email: &str, password: &str) -> (AppState, String) {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    std::mem::forget(container);

    let mut s = AppState::with_pool(pool);
    s.hasher = Arc::new(Hasher::fast_for_tests());
    s.throttle = Arc::new(Throttle::new());
    s.session_key = b"test-key-32-bytes-aaaaaaaaaaaaaa".to_vec();

    let hash = s.hasher.hash(password).unwrap();
    let ws = s.workspaces.as_ref().unwrap().create("default", "W").await.unwrap();
    let u = s.users.as_ref().unwrap().create_local(email, "U", &hash).await.unwrap();
    s.workspaces.as_ref().unwrap().add_member(ws.id, u.id, WorkspaceRole::Owner).await.unwrap();

    // Log in (don't reach into stores again — use the same HTTP path users will).
    let app = router_with_state(s.clone());
    let r = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({"email": email, "password": password}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let sid = r.headers().get("set-cookie").unwrap().to_str().unwrap();
    let sid_kv = sid.split(';').next().unwrap().to_string();
    (s, sid_kv)
}

#[tokio::test(flavor = "multi_thread")]
async fn docs_crud_happy_path() {
    let (state, sid) = login_state("a@x.test", "hunter22").await;
    let app = router_with_state(state);

    // Create.
    let r = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/docs")
                .header("cookie", &sid)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::json!({"title": "Hello"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::CREATED);
    let body = r.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let id = v["id"].as_str().unwrap().to_string();

    // List.
    let r = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/docs")
                .header("cookie", &sid)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let body = r.into_body().collect().await.unwrap().to_bytes();
    let arr: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(arr.as_array().unwrap().len(), 1);

    // Get one.
    let r = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&format!("/api/docs/{id}"))
                .header("cookie", &sid)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let body = r.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["title"], "Hello");
    assert_eq!(v["effective_role"], "owner");
}
```

- [ ] **Step 3: Verify + commit**

```bash
cargo test -p knot-server --test docs_integration
cargo clippy -p knot-server --all-targets --all-features -- -D warnings
git add crates/knot-server/
git commit -m "feat(knot-server): DELETE /api/docs/:id + restore + integration test"
```

---

## Task 15: Grants endpoints

**Files:**
- Create: `crates/knot-server/src/routes/api/grants.rs`
- Modify: `crates/knot-server/src/routes/api/mod.rs` — mount

- [ ] **Step 1: grants.rs**

Create `crates/knot-server/src/routes/api/grants.rs`:

```rust
//! Document grants API:
//! - GET    /api/docs/:id/grants
//! - PUT    /api/docs/:id/grants/:principal   body: {role, inherit}
//! - DELETE /api/docs/:id/grants/:principal

use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, put},
};
use knot_storage::WorkspaceRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::auth::{AuthContext, EffectiveDocRole};
use crate::http_error::json_err;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/docs/:id/grants", get(list))
        .route("/api/docs/:id/grants/:principal", put(put_grant).delete(delete_grant))
}

#[derive(Serialize)]
struct GrantResponse {
    principal: String,
    role: String,
    inherit: bool,
}

async fn list(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    req: Request,
) -> Response {
    let Some(_ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(_role) = req.extensions().get::<EffectiveDocRole>().copied() else {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    };
    let Some(grants) = state.grants.clone() else { return internal() };
    match grants.list(doc_id).await {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|g| GrantResponse {
                    principal: g.principal,
                    role: g.role.as_str().into(),
                    inherit: g.inherit,
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(e) => {
            tracing::error!(error=?e, "grants list");
            internal()
        }
    }
}

#[derive(Deserialize)]
struct PutGrantRequest {
    role: String,
    inherit: bool,
}

async fn put_grant(
    State(state): State<AppState>,
    Path((doc_id, principal)): Path<(Uuid, String)>,
    req: Request,
) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(role) = req.extensions().get::<EffectiveDocRole>().copied() else {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    };
    if role.0 != WorkspaceRole::Owner {
        return json_err(StatusCode::FORBIDDEN, "acl.owner_required", "");
    }
    let Ok(body) = read_json::<PutGrantRequest>(req).await else {
        return json_err(StatusCode::BAD_REQUEST, "bad_request", "");
    };
    let Some(new_role) = WorkspaceRole::parse(&body.role) else {
        return json_err(StatusCode::UNPROCESSABLE_ENTITY, "grant.invalid_role", "");
    };
    if !is_valid_principal(&principal) {
        return json_err(StatusCode::UNPROCESSABLE_ENTITY, "grant.invalid_principal", "");
    }
    let Some(grants) = state.grants.clone() else { return internal() };
    match grants.put(ctx.workspace_id, doc_id, &principal, new_role, body.inherit, ctx.user_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!(error=?e, "grants put");
            internal()
        }
    }
}

async fn delete_grant(
    State(state): State<AppState>,
    Path((doc_id, principal)): Path<(Uuid, String)>,
    req: Request,
) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(role) = req.extensions().get::<EffectiveDocRole>().copied() else {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    };
    if role.0 != WorkspaceRole::Owner {
        return json_err(StatusCode::FORBIDDEN, "acl.owner_required", "");
    }
    let Some(grants) = state.grants.clone() else { return internal() };
    match grants.delete(ctx.workspace_id, doc_id, &principal, ctx.user_id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!(error=?e, "grants delete");
            internal()
        }
    }
}

fn is_valid_principal(s: &str) -> bool {
    // "user:<uuid>" or "group:<non-empty>"
    if let Some(rest) = s.strip_prefix("user:") {
        return Uuid::parse_str(rest).is_ok();
    }
    if let Some(rest) = s.strip_prefix("group:") {
        return !rest.is_empty();
    }
    false
}

async fn read_json<T: serde::de::DeserializeOwned>(req: Request) -> Result<T, ()> {
    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024).await.map_err(|_| ())?;
    serde_json::from_slice(&bytes).map_err(|_| ())
}

fn internal() -> Response {
    json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "")
}
```

- [ ] **Step 2: Mount + middleware note**

Edit `crates/knot-server/src/routes/api/mod.rs`:

```rust
pub mod docs;
pub mod grants;
pub mod workspace;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(workspace::router())
        .merge(docs::router())
        .merge(grants::router())
        .layer(middleware::from_fn(csrf_mw))
        .layer(middleware::from_fn(require_session_mw))
}
```

> **Note:** the docs router applies `require_doc_role_mw` to `/api/docs/:id/*`. Grants routes (`/api/docs/:id/grants` and `/api/docs/:id/grants/:principal`) DON'T match that wildcard automatically — `require_doc_role_mw` was registered via `.route("/api/docs/:id", ...)` only. For Plan 4 the grants routes call `req.extensions().get::<EffectiveDocRole>()` and bail with `acl.no_grant` if missing.
>
> To make the role available to grants too, the docs router needs to layer the middleware on a sub-router containing both routes — or alternatively, the grants router applies the same middleware itself. Pick the cleaner one. The simplest fix is to merge grants INTO the docs router so they share the middleware:

Replace the `pub fn router()` in `crates/knot-server/src/routes/api/docs.rs` to merge grants and apply the doc-role layer to the merged subtree:

```rust
pub fn router() -> Router<AppState> {
    let doc_routes: Router<AppState> = Router::new()
        .route("/api/docs/:id", get(get_one).patch(rename).delete(archive))
        .route("/api/docs/:id/move", post(move_doc))
        .route("/api/docs/:id/restore", post(restore))
        .route("/api/docs/:id/grants", get(crate::routes::api::grants::list_inline))
        .route(
            "/api/docs/:id/grants/:principal",
            put(crate::routes::api::grants::put_inline)
                .delete(crate::routes::api::grants::delete_inline),
        )
        .layer(middleware::from_fn(crate::auth::require_doc_role_mw));
    let list_route: Router<AppState> = Router::new()
        .route("/api/docs", get(list).post(create));
    list_route.merge(doc_routes)
}
```

And in `grants.rs`, expose the handlers as `pub(super)` (rename `list` → `list_inline`, etc.):

```rust
pub(super) async fn list_inline(...) -> Response { ... }
pub(super) async fn put_inline(...) -> Response { ... }
pub(super) async fn delete_inline(...) -> Response { ... }
```

Drop the standalone `pub fn router()` in `grants.rs` and remove `pub mod grants;` from api/mod.rs's `merge` chain (grants now mounts via docs).

- [ ] **Step 3: Verify + commit**

```bash
cargo build -p knot-server
cargo clippy -p knot-server --all-targets --all-features -- -D warnings
git add crates/knot-server/
git commit -m "feat(knot-server): grants endpoints under shared RequireDocRole layer"
```

---

## Task 16: e2e — workspace + docs + grants flow

**Files:**
- Create: `e2e/flows/docs.spec.ts`
- Modify: `e2e/flows/auth.spec.ts` — extend truncate to include documents + document_grants tables

- [ ] **Step 1: Extend the auth.spec truncate**

Edit `e2e/flows/auth.spec.ts`. In `resetAuthTables`, update the TRUNCATE statement:

```ts
    `"TRUNCATE TABLE acl_invalidations, audit_events, document_grants, documents, sessions, workspace_members, users, workspaces CASCADE"`,
```

- [ ] **Step 2: docs.spec.ts**

Create `e2e/flows/docs.spec.ts`:

```ts
import { test, expect, request } from "@playwright/test";
import { execSync } from "node:child_process";

const SERVER = "http://localhost:3000";

function reset() {
  const cmd = [
    "docker compose",
    "-f deploy/compose/dev.yml",
    "exec -T postgres",
    `psql -U knot -d knot -c`,
    `"TRUNCATE TABLE acl_invalidations, audit_events, document_grants, documents, sessions, workspace_members, users, workspaces CASCADE"`,
  ].join(" ");
  execSync(cmd, { cwd: "..", stdio: "pipe" });
}

test.beforeAll(reset);

async function adminCtx() {
  const ctx = await request.newContext({ baseURL: SERVER });
  // Setup the first user (owner).
  const setup = await ctx.post("/auth/setup", {
    data: {
      email: "owner@example.com",
      password: "owner-hunter22",
      display_name: "Owner",
    },
  });
  expect(setup.status()).toBe(201);
  return ctx;
}

test("docs CRUD + grant flow", async () => {
  const ctx = await adminCtx();

  // List empty.
  const empty = await ctx.get("/api/docs");
  expect(empty.status()).toBe(200);
  expect((await empty.json()).length).toBe(0);

  // Create a top-level doc.
  const created = await ctx.post("/api/docs", { data: { title: "Root" } });
  expect(created.status()).toBe(201);
  const root = await created.json();
  expect(root.title).toBe("Root");

  // Create a child.
  const childCreated = await ctx.post("/api/docs", {
    data: { title: "Child", parent_id: root.id },
  });
  expect(childCreated.status()).toBe(201);
  const child = await childCreated.json();

  // Get child with effective_role.
  const got = await ctx.get(`/api/docs/${child.id}`);
  expect(got.status()).toBe(200);
  const body = await got.json();
  expect(body.title).toBe("Child");
  expect(body.effective_role).toBe("owner");

  // PATCH title.
  const renamed = await ctx.patch(`/api/docs/${child.id}`, {
    data: { title: "Renamed" },
  });
  expect(renamed.status()).toBe(200);
  expect((await renamed.json()).title).toBe("Renamed");

  // Move under root → no parent.
  const moved = await ctx.post(`/api/docs/${child.id}/move`, {
    data: { parent_id: null },
  });
  expect(moved.status()).toBe(200);
  expect((await moved.json()).parent_id).toBeNull();

  // Soft-delete and restore.
  const del = await ctx.delete(`/api/docs/${child.id}`);
  expect(del.status()).toBe(204);
  const listAfterDel = await ctx.get("/api/docs");
  expect((await listAfterDel.json()).find((d: any) => d.id === child.id)).toBeUndefined();

  const restored = await ctx.post(`/api/docs/${child.id}/restore`);
  expect(restored.status()).toBe(204);

  // Grant another user editor; verify list.
  // (Use admin path to create a 2nd user; v0.1 has no email invite, so we
  // insert via direct SQL through the admin compose stack.)
  execSync(
    `docker compose -f deploy/compose/dev.yml exec -T postgres psql -U knot -d knot -c ` +
      `"INSERT INTO users (email, display_name) VALUES ('bob@example.com', 'Bob')"`,
    { cwd: "..", stdio: "pipe" },
  );
  const otherUser = await ctx.get("/api/workspace/members");
  // Members list still shows only owner (bob isn't a workspace member yet).
  expect((await otherUser.json()).length).toBe(1);

  // Add bob via invite.
  const invite = await ctx.post("/api/workspace/members", {
    data: { email: "bob@example.com", role: "viewer" },
  });
  expect(invite.status()).toBe(201);
  const members = await ctx.get("/api/workspace/members");
  expect((await members.json()).length).toBe(2);
  const bob = (await members.json()).find((m: any) => m.email === "bob@example.com");

  // Grant bob editor on the root doc.
  const principal = `user:${bob.user_id}`;
  const put = await ctx.put(
    `/api/docs/${root.id}/grants/${encodeURIComponent(principal)}`,
    { data: { role: "editor", inherit: true } },
  );
  expect(put.status()).toBe(204);

  const grantsList = await ctx.get(`/api/docs/${root.id}/grants`);
  expect(grantsList.status()).toBe(200);
  const arr = await grantsList.json();
  expect(arr.length).toBe(1);
  expect(arr[0].role).toBe("editor");

  // Delete grant.
  const delGrant = await ctx.delete(
    `/api/docs/${root.id}/grants/${encodeURIComponent(principal)}`,
  );
  expect(delGrant.status()).toBe(204);
  const empty2 = await ctx.get(`/api/docs/${root.id}/grants`);
  expect((await empty2.json()).length).toBe(0);
});
```

- [ ] **Step 3: Run all e2e**

```bash
cd /home/nik/Development/knot
make compose.up
make migrate.up
cd e2e
pnpm playwright test
```

Expected: all 5 e2e tests pass (health + 2 auth + convergence + docs).

- [ ] **Step 4: Commit**

```bash
git add e2e/
git commit -m "test(e2e): workspace + docs + grants happy-path flow"
```

---

## Self-review checklist (for the executing agent)

Before declaring Plan 4 complete:

- [ ] `cargo test --workspace` fully green.
- [ ] `cd e2e && pnpm test` fully green (5 suites).
- [ ] `cargo deny check` fully green.
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
- [ ] LISTEN/NOTIFY pipeline works end-to-end: change a grant, verify the cache evicts (covered by `crates/knot-docs/tests/listener_integration.rs`).
- [ ] `/api/docs/:id` returns `effective_role` in the body.
- [ ] PATCH/DELETE/restore enforce role minimums (Editor / Owner).
- [ ] Last-owner check prevents demoting/removing the only Owner.
- [ ] Grants endpoints validate principal format ("user:<uuid>" or "group:<name>").
- [ ] Plan 3 carryovers — `Config::oidc_auto_provision` is the source of truth; existing-OIDC-user grant is gated; `WorkspaceStoreError::NotFound` is gone.

When all boxes check: write `docs/superpowers/research/2026-06-0X-plan4-outcome.md` summarising what landed + any spec drift, tag `plan-4-complete`, and proceed to Plan 5 (CRDT room actor + persistence).
