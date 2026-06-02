# Plan 6 Outcome — Frontend Shell

**Status:** GO. All 21 tasks landed; all gates green.

**Verdict:** Continue to Plan 7 (UI polish or auth completion). The frontend now flows through real auth + lazy editor + collab WS with the same data-shape the server expects.

## What landed

Plan 6 commits, oldest to newest (HEAD `670b420`):

| Commit | Task | Subject |
|---|---|---|
| 94fc3ca | T1  | toolchain hardening — deps + ESLint flat config + Vitest |
| 7ec6c20 | T2  | API client + CSRF + ApiError + valibot validators (+4 unit tests) |
| 0f57578 | T3  | QueryClient + Zustand UI store + AppShell + Toast + StatusDot + ErrorBoundary |
| 80deff0 | T4  | SessionContext + RequireAuth route gate |
| ad08118 | T5  | React Router v6 + lazy routes + entrypoint rewrite |
| 0df3c82 | T6  | LoginPage with email/password + OIDC link |
| 51be472 | T7  | SetupPage for first-run admin creation |
| 0974fdb | T8  | docsApi + tree builder + tree unit tests (4 tests) |
| 42f8a4f | T9  | DocTree sidebar with create + select |
| 549f34d | T11 | KnotProvider — custom y-protocol v1 WS client (+2 unit tests) |
| bd8fb07 | T12 | Tiptap editor with canonical schema + collaboration cursor |
| 03e5792 | T10 | DocPage shell with title + status dot + lazy editor |
| 6d98d51 | T13 | rename + archive via right-click prompt |
| ad0b76e | T14 | presence bar above editor |
| 51107f0 | T15 | 4403 close → toast + redirect to landing |
| 6790aed | T16 | MembersPage with list/invite/role/remove |
| 024670e | T17 | PermissionsDialog over /doc/:id/permissions |
| d5aefa6 | T18 | SettingsPage with workspace info + logout |
| f2c10ce | T19 | landing redirects to first doc; onboarding card otherwise |
| cb77fc3 | T20+T21 | e2e: login + editor + re-enabled two-users-converge |
| 670b420 | post-fix | StrictMode-safe editor + single-Outlet layout |

Tasks were executed in plan order with one reorder (T11 + T12 before T10 because DocPage imports the new KnotEditor signature) and one post-fix commit that resolved bugs caught by e2e.

## Gates

- `pnpm tsc` — clean
- `pnpm lint` — clean (zero warnings)
- `pnpm test` (vitest) — 10 / 10 pass (api: 4, tree: 4, KnotProvider: 2)
- `pnpm playwright test` — 10 / 10 pass (auth, collab, docs, editor, health, login × 3, two-users-converge)
- `cargo test --workspace` — unchanged (no server-side changes in Plan 6)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — unchanged

## Architecture

The three state sources from spec §10.2 are now properly separated:

- **Server state** → TanStack Query. Keys: `["session"]`, `["docs"]`, `["doc", id]`, `["workspace"]`, `["members"]`, `["grants", docId]`.
- **Doc body** → Y.Doc + KnotProvider (per-doc WebSocket). The provider owns the WS, decodes the y-protocol v1 wire format directly (no `y-websocket`), and maps WebSocket close codes onto a five-state lifecycle (`connecting | connected | offline | unauthorised | conflict`).
- **UI state** → Zustand. Sidebar open/closed, toast stack.

Auth is a route-loader: `SessionProvider` runs a query against `/auth/session`, `RequireAuth` redirects to `/login` on 401, and `LoginPage` invalidates the session query on success. The cookie is the truth — no client-side auth state.

Routing tree (post-fix shape):

```
/login                            → LoginPage
/setup                            → SetupPage
RequireAuth
  └─ AppShell  (sidebar = DocTree, main = Outlet)
     ├─ /                         → Landing (auto-redirect to first doc)
     ├─ /doc/:id                  → DocPage  (Outlet for permissions overlay)
     │   └─ permissions           → PermissionsDialog (modal over DocPage)
     ├─ /members                  → MembersPage
     └─ /settings                 → SettingsPage
*                                  → Navigate("/")
```

The pre-fix shape had two `<Outlet />`s in AppShell (one in sidebar, one in main), which React Router renders identically. Every protected route was therefore rendered twice. Fixed in the post-fix commit.

## Bugs caught and fixed

1. **Double-render via AppShell's two Outlets** — every route element rendered twice (one in `<aside>`, one in `<main>`), so the editor mounted twice and there were two `data-testid="new-doc"` buttons. Fixed by putting `DocTree` directly in the sidebar and giving `<main>` a single Outlet. DocPage gets its own Outlet for the permissions overlay.

2. **React StrictMode leaked a duplicate WebSocket** — the original `KnotEditor` created `Y.Doc` and `KnotProvider` inside `useMemo`. React 18 StrictMode double-invokes useMemo factories in dev, creating two providers, but the effect only attached its status listener to the second one. The first provider's WS opened with zero listeners; the SECOND provider's WS was closed during StrictMode's unmount cycle. Status stayed at `"connecting"` forever even though packets were flowing. Fixed by moving Y.Doc + provider creation into a `useEffect` and splitting the editor into a shell + body component so Tiptap's `useEditor` only mounts after the doc/provider pair is committed.

3. **`extensions: []` crashed Tiptap** — when the editor mounted before the provider, `useEditor({ extensions: [] })` threw "Schema is missing its top node type ('doc')". The split-component fix in #2 sidesteps this; `useEditor` only runs in `EditorBody`, which only mounts when the pair exists.

4. **Vite proxy missing /api and /auth** — the spike's `vite.config.ts` only proxied `/collab`. The new SPA hits `/api/*` and `/auth/*` via relative URLs. Added.

## What's still deferred

Carried forward in the plan, intentionally not in Plan 6:

- **Real two-user convergence** in `two-users-converge.spec.ts` — needs invite-with-password (Plan 8). The current test verifies the single-user WS + persistence loop after reload.
- **Permission-aware editor toolbar** (per spec §10.6 — Editor+/Viewer toolbar variants).
- **Drag-drop tree move** (`docsApi.move` is exposed but no UI).
- **Command palette** (Zustand store has the slot; no UI in Plan 6).
- **OIDC button proper styling / config UI** — login has a `<a href="/auth/oidc/login">` link only.
- **Bundle-budget CI enforcement** — manual review for v0.1.
- **Mobile / responsive polish** — desktop-first.

## Carryforward for the next plan

A future Plan 7 should pick from:

- UI polish (drag-drop tree, command palette, real context menu, toolbar)
- OR Plan 8 (auth completion: invite-with-password, password reset)
- OR Plan 9 (deployment: Helm chart + multi-arch image build)

Recommendation: **Plan 8** next, because the deferred two-user e2e test in Plan 6 (re-enabled but only single-user) can finally become honest after invite-with-password lands. UI polish is independent and can interleave.

## Files of interest

| Path | Role |
|---|---|
| `web/src/lib/api.ts` | Single fetch wrapper. CSRF + ApiResult<T>. |
| `web/src/lib/validators.ts` | Valibot schemas for all server responses. |
| `web/src/features/editor/KnotProvider.ts` | Custom y-protocol v1 client with five-state lifecycle. |
| `web/src/features/editor/KnotEditor.tsx` | Shell + body split for StrictMode safety. |
| `web/src/routes.tsx` | Lazy-loaded route tree. |
| `web/src/components/AppShell.tsx` | Sidebar = DocTree, main = single Outlet. |
| `e2e/flows/editor.spec.ts` | New: end-to-end editor smoke + reload-persistence. |
| `e2e/flows/login.spec.ts` | New: setup, redirect-anon, wrong-password. |
| `e2e/flows/two-users-converge.spec.ts` | Re-enabled (single-user v0.1). |
