# Public Share Links Implementation Plan (Plan 17)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development (recommended) or superpowers:executing-plans.

**Goal:** Notion-style anonymous read-only share links. Owner enables on a doc, gets a URL like `https://knot.example.com/p/<token>`, sends it to anyone — they read the doc without logging in. Owner can set an expiry and revoke at any time.

**Architecture:**
- **Schema:** `share_tokens(id PK, token TEXT UNIQUE, doc_id FK, workspace_id, expires_at TIMESTAMPTZ NULL, revoked_at TIMESTAMPTZ NULL, created_by, created_at)`. Token is URL-safe base64 of 24 random bytes (≈32 chars). Unique constraint enforces collision avoidance.
- **Auth surface:** the public route `GET /p/:token` is excluded from the session middleware. The handler validates token + checks `revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())`, then serves cached markdown rendered as HTML server-side (via the already-bundled `pulldown_cmark`). No CSRF (idempotent GET, no cookies issued).
- **Render:** markdown → HTML on the server. The frontend's `/p/:token` route fetches the rendered HTML and `dangerouslySetInnerHTML`s it. Sanitizer applied to pulldown_cmark output is unnecessary because the input markdown is workspace-trusted (only Owners can publish) — but we still apply a conservative XSS escape on the title and frame.
- **Owner UX:** `PermissionsDialog` gains a "Public link" section: toggle (creates a token), expiry picker (datetime-local), the URL + copy button, revoke button. State: TanStack Query of `["shares", docId]` against the new endpoints.

**Predecessor:** Plan 15 (HEAD `62925c9`).

**Out of scope:**
- **Comment-allowed tokens.** Defer until Plan 19 (comments) lands.
- **Token-based collab editing.** Read-only only.
- **Custom URL slugs.** Auto-generated tokens.
- **Email or password protection.** Bare URL is the auth.
- **Multiple expiry presets in UI.** Datetime picker only.
- **OG/Twitter card meta tags.** Worth adding once knot grows; placeholder noted but not implemented.
- **Rate limit on /p/:token.** Existing global throttle covers it; bespoke per-token rate limit is hardening.

---

## File map

```
migrations/
└── <ts>_share_tokens.sql                                  (new) table + indexes

crates/knot-storage/
└── src/share_tokens.rs                                    (new) trait + Pg impl + DTOs

crates/knot-server/
├── src/lib.rs                                             (modify) wire SharesStore into AppState
├── src/auth/require_session.rs                            (modify) exempt /p/:token
├── src/routes/api/shares.rs                               (new) POST/GET/DELETE /api/docs/:id/shares
├── src/routes/public.rs                                   (new) GET /p/:token (no auth)
└── tests/shares_integration.rs                            (new) owner-only + ACL + expiry + revoke
                                                                 + anon GET works
                                                                 + revoked → 410

web/
└── src/
    ├── lib/shares.api.ts                                  (new) sharesApi
    ├── features/permissions/PermissionsDialog.tsx         (modify) "Public link" section
    ├── features/public/PublicDoc.tsx                      (new) /p/:token route
    ├── routes.tsx                                         (modify) add /p/:token outside RequireAuth
    └── App-level CSS                                       (n/a — inline styles)

e2e/flows/
└── share-link.spec.ts                                     (new) enable → anon read → revoke
```

---

## Conventions

- **Token format:** 24 random bytes via `rand::rngs::OsRng` (already transitively in the tree), base64url-encoded (no padding). Length ≈32 chars. Bytea would be denser but URL handling is uglier.
- **Expiry:** `TIMESTAMPTZ` nullable. Frontend sends ISO-8601; server stores UTC.
- **Render path:** markdown → HTML via `pulldown_cmark::html::push_html(&mut buf, parser)`. Wrapped in a minimal HTML skeleton with the doc title.
- **Auth exemption:** `require_session` middleware checks `if path.starts_with("/p/")` then skips. Same pattern as the existing `/api/healthz` exemption (verify the helper).
- **Cache headers on `/p/:token`:** `Cache-Control: public, max-age=60`. Tokens are stable; one minute lets a viral share survive bursts.
- **Revoke is one-way:** sets `revoked_at = now()`. No "un-revoke" UI. A future owner can mint a fresh token.

---

## Task overview

| # | Title | LOC ≈ |
|---|---|---|
| 1 | Migration: share_tokens table | 50 |
| 2 | knot-storage: ShareTokenStore trait + Pg impl | 180 |
| 3 | Auth middleware: exempt /p/* | 40 |
| 4 | Server: POST/GET/DELETE /api/docs/:id/shares | 200 |
| 5 | Server: GET /p/:token public render | 160 |
| 6 | Server integration tests | 260 |
| 7 | web: sharesApi | 80 |
| 8 | web: PermissionsDialog "Public link" section | 220 |
| 9 | web: PublicDoc route + routes.tsx wiring | 130 |
| 10 | e2e: share-link spec | 140 |
| 11 | Outcome doc | 0 |

---

## Task 1: Migration

```bash
make migrate.create NAME=share_tokens
```

`migrations/<ts>_share_tokens.sql`:

```sql
-- share_tokens
-- Created 2026-06-03

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
CREATE INDEX share_tokens_doc_idx ON share_tokens(doc_id) WHERE revoked_at IS NULL;
```

Apply + commit.

---

## Task 2: ShareTokenStore

`crates/knot-storage/src/share_tokens.rs` (mirror BlobStore / SearchStore pattern):

```rust
use async_trait::async_trait;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ShareStoreError {
    #[error("not found")]
    NotFound,
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ShareToken {
    pub id: Uuid,
    pub token: String,
    pub workspace_id: Uuid,
    pub doc_id: Uuid,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_by: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[async_trait]
pub trait ShareTokenStore: Send + Sync {
    async fn create(&self, workspace_id: Uuid, doc_id: Uuid, expires_at: Option<chrono::DateTime<chrono::Utc>>, created_by: Uuid) -> Result<ShareToken, ShareStoreError>;
    async fn list_active(&self, doc_id: Uuid) -> Result<Vec<ShareToken>, ShareStoreError>;
    async fn find_by_token(&self, token: &str) -> Result<Option<ShareToken>, ShareStoreError>;
    async fn revoke(&self, id: Uuid) -> Result<(), ShareStoreError>;
}

pub struct PgShareTokenStore { pool: PgPool }
impl PgShareTokenStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl ShareTokenStore for PgShareTokenStore {
    // create: generates 24-byte random token, base64url-encodes, INSERT, returns the row.
    // list_active: WHERE doc_id = $1 AND revoked_at IS NULL ORDER BY created_at DESC.
    // find_by_token: WHERE token = $1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now()) LIMIT 1.
    // revoke: UPDATE share_tokens SET revoked_at = now() WHERE id = $1.
}
```

Use `rand::rngs::OsRng` (knot-auth already pulls `rand`) + `base64::engine::general_purpose::URL_SAFE_NO_PAD`. Add `base64.workspace = true` to knot-storage if not present.

Verify + commit.

---

## Task 3: Exempt /p/* from session middleware

`crates/knot-server/src/auth/require_session.rs` — check the current path-exempt list. Add `/p/` prefix to the bypass.

Quick sniff before editing: `grep -n "starts_with\|exempt\|/api/health" crates/knot-server/src/auth/require_session.rs`.

Verify + commit (or fold into T5's commit).

---

## Task 4: Owner-side endpoints

`crates/knot-server/src/routes/api/shares.rs`:

```rust
POST   /api/docs/:doc_id/shares            { expires_at? } → 201 { id, token, url, expires_at }
GET    /api/docs/:doc_id/shares            → 200 [{ id, token, url, expires_at, created_at }, ...]
DELETE /api/docs/:doc_id/shares/:share_id  → 204
```

ACL: Owner on the doc (or workspace Owner) required for all three. Mirror the existing `blobs.rs` ACL pattern: `acl.effective_role(workspace, doc_id, user_id)` must be `Owner`.

`url` field is `format!("{}/p/{}", cfg.base_url, token)`.

Mount in `routes/api/mod.rs`.

---

## Task 5: Public GET /p/:token

`crates/knot-server/src/routes/public.rs`:

```rust
GET /p/:token
  - look up share_token; not found OR expired OR revoked → 410 Gone with HTML
  - look up doc_markdown_cache; cache miss → 503 with "still rendering" placeholder
  - render markdown → HTML via pulldown_cmark
  - wrap in minimal HTML skeleton (escape title via askama_escape or hand-rolled)
  - return 200 text/html with Cache-Control: public, max-age=60
```

Mount under `Router::new().route("/p/:token", get(public_doc))` and merge into the top-level router OUTSIDE the auth middleware layer (verify by reading `lib.rs::router_with_state`).

---

## Task 6: Server integration tests

`crates/knot-server/tests/shares_integration.rs` — cases:

1. Owner creates a token → 201 with non-empty token + url.
2. Editor tries to create → 403.
3. Viewer tries to create → 403.
4. Anon `GET /p/<token>` returns 200 HTML containing the doc title.
5. Anon `GET /p/<bogus>` → 410.
6. Owner revokes → anon GET → 410.
7. Token with `expires_at` in the past → 410.
8. Token with `expires_at` in the future → 200.
9. Doc with no markdown cache row → 503.

---

## Task 7: web — sharesApi

`web/src/lib/shares.api.ts`:

```ts
export type Share = {
  id: string;
  token: string;
  url: string;
  expires_at: string | null;
  created_at: string;
};

export const sharesApi = {
  list(docId: string)                                  → ApiResult<Share[]>
  create(docId: string, expiresAt: string | null)      → ApiResult<Share>
  revoke(docId: string, shareId: string)               → ApiResult<void>
};
```

Stylistic mirror of `grants.api.ts`.

---

## Task 8: PermissionsDialog "Public link"

Modify `web/src/features/permissions/PermissionsDialog.tsx`. Add a section above the existing Grants table:

```
┌────────────────────────────────────────────┐
│ Public link                                │
│   [×] Anyone with the link can read        │
│                                            │
│   URL:  https://.../p/abc123… [📋 copy]    │
│   Expires:  [datetime-local]               │
│   Created:  2026-06-03 by Alice            │
│   [Revoke]                                 │
└────────────────────────────────────────────┘
```

- Toggle ON → create a token (default no expiry); toggle OFF → revoke.
- TanStack Query keys: `["shares", docId]`.
- Datetime change: revoke + recreate with new expiry (simpler than PATCH for v0.1).
- Copy button uses `navigator.clipboard.writeText`.

---

## Task 9: PublicDoc route

`web/src/features/public/PublicDoc.tsx`:

```tsx
export default function PublicDoc() {
  const { token } = useParams<{ token: string }>();
  const q = useQuery({
    queryKey: ["public", token],
    queryFn: async () => {
      const r = await fetch(`/p/${token}`, { credentials: "omit" });
      return { status: r.status, html: await r.text() };
    },
  });
  if (q.isLoading) return <div>Loading…</div>;
  if (!q.data || q.data.status === 410) return <div>Link expired or revoked.</div>;
  if (q.data.status === 404) return <div>Not found.</div>;
  return <article style={{ maxWidth: 720, margin: "40px auto", padding: 24 }}
                  dangerouslySetInnerHTML={{ __html: q.data.html }} />;
}
```

Wire into `routes.tsx` ABOVE the `RequireAuth` block so it never gates on a session:

```tsx
{ path: "/p/:token", element: <Lazy><PublicDoc /></Lazy> },
```

---

## Task 10: e2e share-link spec

`e2e/flows/share-link.spec.ts`:

1. Owner sets up, creates a doc, opens PermissionsDialog, toggles "Public link" ON.
2. Captures the URL from the input.
3. Opens a fresh `browser.newContext()` (no cookies), visits the URL → sees the doc content.
4. Returns to owner, clicks Revoke.
5. Anon revisits → "Link expired or revoked."

---

## Task 11: Outcome doc + README row

Same shape as prior outcome docs. Status, gates, what landed, what's deferred, carryforward.

---

## Self-review

- [ ] `cargo test --workspace` green (+9 shares integration cases)
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [ ] `pnpm tsc/lint/test` clean
- [ ] `pnpm playwright test` green (24+/24+)
- [ ] Bundle stayed flat
- [ ] Manual: owner enables share, copies URL, opens in incognito, sees doc; revokes, incognito refresh → "Link expired or revoked"
- [ ] Manual: set expiry to "yesterday" via devtools, anon GET → 410
