# Plan 17 Outcome — Public Share Links

**Status:** GO. All 11 tasks landed. **24/24 e2e pass.**

**Verdict:** knot is no longer team-only. An owner can flip a doc public, share the URL, and anyone — no account, no login — can read it. Revocation and expiry both work. This is the biggest product unlock since v0.1 shipped.

## What landed

Plan 17 commits (HEAD `457b81c`):

| Commit | Task | Subject |
|---|---|---|
| 4e46111 | T1 | migrations: share_tokens table + partial index for active tokens |
| f956df7 | T2 | knot-storage: ShareTokenStore + PgShareTokenStore |
| 563645b | T3 | knot-server: exempt /p/* from session middleware |
| 6473ec8 | T4 | knot-server: POST/GET/DELETE /api/docs/:id/shares |
| d3a7e52 | T5 | knot-server: GET /p/:token — anonymous markdown render |
| e234330 | T6 | test(knot-server): 9 integration cases |
| 034a1f2 | T7 | web: sharesApi (list/create/revoke) |
| 9149f56 | T8 | web: PermissionsDialog public-link section |
| e445e8c | T9 | web: PublicDoc route at /p/:token (outside auth gate) |
| 457b81c | T10 | e2e: anon read + revoke + Vite /p proxy + no-store |

T11 is this outcome doc.

## Gates

- `cargo test --workspace` — green (+9 shares cases)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean
- `pnpm tsc/lint/test` — clean
- `pnpm playwright test` — **24 passed, 0 skipped**
- `make image.build.host` produces a working image; manual smoke: enable share → curl /p/<token> → text/html

## Architecture summary

**Schema:** `share_tokens(id, token UNIQUE, workspace_id, doc_id, expires_at NULL, revoked_at NULL, created_by, created_at)` with a partial index `WHERE revoked_at IS NULL` for the hot list-active path. Token is URL-safe base64 of 24 random bytes (~32 chars).

**Auth surface:**
- `/api/docs/:doc_id/shares` POST/GET/DELETE — Owner-only on the doc.
- `/p/:token` GET — anonymous. The session middleware short-circuits on `path.starts_with("/p/")`. Defense in depth: the route is also mounted at the top-level router OUTSIDE the API router that layers the session middleware.
- Token IS the authentication. No CSRF (idempotent GET, no cookies issued, no state mutation).

**Render path** (Rust):
- `find_alive(token)` enforces `revoked_at IS NULL AND (expires_at IS NULL OR expires_at > NOW())`.
- Markdown comes from `doc_markdown_cache.markdown_text`. Empty cache → 503 with Retry-After.
- `pulldown_cmark::html::push_html` does the markdown → HTML conversion. Output wrapped in a minimal `<!doctype>` skeleton with the doc title in `<title>`, a `<meta viewport>`, and an inline `<style>` for sensible reading defaults (centered article, 720px max width, mono-font code blocks).
- `Cache-Control: public, max-age=60` on success — gentle CDN-friendly cache.

**Render path** (web):
- `/p/:token` route is registered BEFORE the `RequireAuth` block in `routes.tsx`.
- `PublicDoc` fetches the server's HTML with `cache: "no-store"` and embeds via `<iframe srcDoc>` with `sandbox="allow-same-origin"`. The iframe isolates the server's inline `<style>` from the SPA's frame.
- Vite dev-proxy gets a `/p` entry that bypasses to the SPA when `Accept: text/html` (browser navigation) and forwards to knot-server otherwise (the SPA's own fetch).

**Permissions UX:**
- New section in `PermissionsDialog` above the Grants table.
- Off: a single button "Enable public link".
- On: read-only URL input, copy button (uses `navigator.clipboard`), datetime-local expiry picker with Save button, "Created … by …" footer, and a Revoke button.
- Expiry edit = revoke + recreate (simpler than a PATCH endpoint for v0.1).

## What was non-obvious

**Vite dev-proxy needs `/p` too.** The first e2e run failed because PublicDoc's `fetch('/p/<token>')` came back as the SPA's own `index.html` — Vite served it. Added a `/p` proxy entry with a `bypass` callback that returns `req.url` (= serve SPA) when `Accept` includes `text/html`, otherwise forwards to `:3000`. Browser navigation goes one way, `fetch()` goes the other.

**`cache: "no-store"` is mandatory.** The server sends `Cache-Control: public, max-age=60` so CDNs and browsers can cache. But the SPA's `PublicDoc` needs to surface a freshly-revoked token immediately — without `cache: "no-store"` the browser replays the cached 200 and the user never sees the 410. Lesson: server-side cache headers are for the world; client-side fetch options are for the SPA.

**Public route mounted BEFORE the API router.** First wiring of the public route went next to the API merge and inherited the session middleware layer. Auth middleware then 401'd anonymous requests before `/p/:token`'s `path.starts_with("/p/")` check even ran. Fixed by merging the public router at the top-level `router_with_state` builder, outside the auth-layered API sub-router. The auth middleware's `/p/*` exemption is now defense-in-depth, not the primary gate.

**Doc title goes in `<title>`, not the body.** First e2e assertion checked for the doc title inside the `<body>` and timed out. The server skeleton renders title in `<title>` (browser tab) and content in `<article>` from the markdown render. Fixed the assertion to look at `article`.

**Yjs WS frames flush asynchronously.** The e2e types into the editor and then calls the markdown export endpoint. First runs hit 503 from the public route because the WS frames hadn't reached the room actor when the export ran. Wrapped the export call in `expect.poll(...)` so the test retries until the export sees the typed content.

**iframe srcdoc requires same-origin sandbox.** Without `sandbox="allow-same-origin"`, the iframe can't read its own `document` and Playwright can't query inside it. With it, the iframe is properly isolated from the SPA's CSS but Playwright can still introspect — what we want.

**`MarkdownCacheStore::get(doc_id)` was added.** Plan 5 only had `get_if_fresh(doc_id, seq)`. The public render needs the cache even when its seq is behind — partial staleness is OK for v0.1. Added a seq-agnostic `get` to the trait + Pg impl in T5.

**`knot-test-support` migration cache stale.** Same pattern as Plan 13: `sqlx::migrate!` embeds the migration list at compile time, so adding the `share_tokens` migration required `cargo clean -p knot-test-support` before the integration tests could see the table. Caught in T6.

## What's still deferred

- **Comment-allowed tokens.** Anonymous read-only only; defer until Plan 19 (comments).
- **Token-based collab editing.** No write access via tokens. Probably never — it'd complicate the auth model significantly.
- **Custom URL slugs.** Auto-generated tokens only.
- **Email or password protection on a token.** Single-secret URL is the auth.
- **Multiple tokens per doc in the UI.** The store supports it; the dialog shows only the first active token. A future plan can add a list view.
- **OG / Twitter card meta tags.** Nice for social sharing. Placeholder is a single `<title>`.
- **Rate limit on `/p/:token`.** Leans on the global throttle. A per-token limit (e.g. 1000 req/hour) is hardening.
- **Token rotation on expiry.** Currently revoke + recreate (manual). Auto-rotate-on-expiry would be a small UX add.
- **Expiry display in the user's local timezone.** The datetime-local input already does this; the display text uses `toLocaleString` which respects locale.
- **Public images / blobs.** The render uses cached markdown; if the doc embeds a `/api/blobs/<id>` URL, anon visitors can't load it (ACL check fails). Out of scope for v0.1; would need a "public-by-association" rule on blobs whose parent doc has an active share.

## Carryforward

Continue with the user's batch list:
1. **Plan 20 — Doc history / time travel** (~8 tasks, leans on existing Yjs persistence).
2. **Plan 19 — Comments / inline mentions** (~15 tasks, biggest remaining).

## Files of interest

| Path | Role |
|---|---|
| `migrations/20260603140829_share_tokens.sql` | table + partial index |
| `crates/knot-storage/src/share_tokens.rs` | `ShareTokenStore` + Pg impl + token gen |
| `crates/knot-server/src/auth/require_session.rs` | `/p/*` exemption |
| `crates/knot-server/src/routes/api/shares.rs` | owner endpoints (POST/GET/DELETE) |
| `crates/knot-server/src/routes/public.rs` | anonymous render with pulldown_cmark |
| `crates/knot-server/tests/shares_integration.rs` | 9 cases (owner/ACL/anon/expiry/revoke/no-cache) |
| `web/src/lib/shares.api.ts` | client wrapper |
| `web/src/features/permissions/PermissionsDialog.tsx` | "Public link" section |
| `web/src/features/public/PublicDoc.tsx` | /p/:token route, iframe srcdoc |
| `web/src/routes.tsx` | `/p/:token` outside RequireAuth |
| `web/vite.config.ts` | `/p` proxy with HTML-accept bypass |
| `e2e/flows/share-link.spec.ts` | enable → anon read → revoke → 410 |
