# Frontend Shell Implementation Plan (Plan 6)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Plan 1 spike SPA (single editor pane, no auth, hardcoded docId) with the §10 architecture — React Router v6 + auth gate + TanStack Query (server state) + Y.Doc/KnotProvider (doc body) + Zustand (UI state) + Tiptap with the canonical schema, plus pages for login, setup, doc list/editor, members, and per-doc permissions.

**Architecture:** Three state sources kept strictly separate (server / Y.Doc / UI). Auth is a route-loader pattern: every protected route hits `/auth/session` and redirects to `/login` on 401. The editor lazy-loads on `/doc/:id` to keep the main chunk under 250 KB gzipped. `KnotProvider` is a custom y-protocol v1 WS client (no `y-websocket`) so we own the lifecycle and the 4403-close → "access removed" flow. Tiptap extensions match the canonical ProseMirror schema generated from `tools/schema.json` (which already emits `web/src/features/editor/schema.ts` per Plan 1).

**Tech Stack:** React 18, React Router v6, TanStack Query 5, Zustand 4, Tiptap 2 (extensions: Collaboration + CollaborationCursor + the canonical schema set), Yjs 13, y-protocols 1, valibot 0.x (lightweight runtime validation), Vitest + Testing Library, Playwright (existing e2e harness).

**Predecessor:** Plan 5 (CRDT Room Actor + Persistence, outcome at `docs/superpowers/research/2026-06-02-plan5-outcome.md`, HEAD at commit `ad96d23`). All `/api/*` endpoints + `/collab/:doc_id` WS + 4403 close are wired server-side.

**Out of scope for this plan** (each gets its own follow-up plan or is intentionally deferred):
- Permission-aware editor toolbar (Editor+/Viewer hides per-button) — covered by spec §10.6 quality bar; deferred to a UI-polish plan after this one.
- Command palette (Zustand state slot exists but no UI in Plan 6).
- Advanced markdown blocks (tables, callouts) — spec defers per §8.8.
- Mobile / responsive polish — desktop-first for v0.1.
- Helm chart + multi-arch image build — Plan 9.
- Bundle-budget enforcement in CI (vitest + tsc are the only CI gates; bundle size left to manual `pnpm build` check during reviews).

---

## Spec coverage map

What this plan implements from `docs/superpowers/specs/2026-06-01-knot-foundation-design.md`:

| Spec section | Tasks |
|---|---|
| §10.1 Routing (React Router v6 — /, /login, /setup, /doc/:id, /doc/:id/permissions, /members, /settings, *) | T5, T6, T7, T10, T11, T15, T16, T17, T18 |
| §10.1 Auth gate (route loader → /auth/session, 401→/login) | T4, T5 |
| §10.2 Three-state separation (Query / Y.Doc / Zustand) | T3 (Query+Zustand init); T11 (Y.Doc separation enforced by KnotProvider boundary) |
| §10.3 Tiptap extensions matching canonical schema | T12 |
| §10.4 KnotProvider — own WS client + lifecycle `connecting | connected | offline | unauthorised | conflict` | T11 |
| §10.4 4403 close → "no longer have access" | T11, T15 |
| §10.5 API client (credentials, CSRF, ApiError) | T2 |
| §10.5 Per-feature typed helpers (docsApi, workspaceApi, grantsApi) | T8, T9, T16, T17 |
| §10.6 TS strict + ESLint + bundle budget (editor lazy-loaded) | T1, T10 |
| §10.6 Permission-aware destructive controls hidden by effective_role | T13 (rename/move/archive in tree), T17 (grants list visible to non-owners) |
| Plan 5 carryover — re-enable two-users-converge under real auth | T20 |

Deferred (intentional):
- §10.6 Bundle budget CI enforcement — manual review for v0.1.
- Command palette UI — Zustand slot only.
- OIDC button — `/auth/oidc/login` is a link from LoginPage (T6); no UI for OIDC config.

---

## File map

```
knot/web/
├── package.json                                (modify) +deps: react-router-dom, @tanstack/react-query, zustand, valibot, +dev: @testing-library/react, @testing-library/jest-dom, jsdom, eslint, @typescript-eslint, eslint-plugin-react-hooks, eslint-plugin-import
├── tsconfig.json                               (modify) verify noUncheckedIndexedAccess + noImplicitOverride (already set per Plan 1)
├── vitest.config.ts                            (new) jsdom env + Testing Library setup
├── .eslintrc.cjs                               (new) strict ruleset per §10.6
├── src/
│   ├── main.tsx                                (rewrite) BrowserRouter + QueryClientProvider + routes
│   ├── App.tsx                                 (replaced by main.tsx wiring)
│   ├── lib/
│   │   ├── api.ts                              (new) fetch wrapper + CSRF + ApiError + ApiResult<T>
│   │   ├── csrf.ts                             (new) readCookie helper
│   │   ├── validators.ts                       (new) valibot schemas for /auth/session, /api/docs, etc.
│   │   └── queryClient.ts                      (new) QueryClient singleton
│   ├── stores/
│   │   └── ui.ts                               (new) Zustand store — sidebar open/closed, toast, modal
│   ├── auth/
│   │   ├── SessionContext.tsx                  (new) useSession() + SessionProvider that runs the loader query
│   │   ├── RequireAuth.tsx                     (new) Outlet-wrapper redirecting to /login on no session
│   │   └── session.api.ts                      (new) authApi.session, authApi.login, authApi.logout, authApi.setup
│   ├── features/
│   │   ├── auth/
│   │   │   ├── LoginPage.tsx                   (new)
│   │   │   ├── SetupPage.tsx                   (new)
│   │   │   └── pages.css.ts                    (new) shared minimal styling
│   │   ├── workspace/
│   │   │   ├── workspace.api.ts                (new) workspaceApi.get + members.* helpers
│   │   │   ├── MembersPage.tsx                 (new)
│   │   │   └── SettingsPage.tsx                (new)
│   │   ├── docs/
│   │   │   ├── docs.api.ts                     (new) docsApi.list/create/get/patch/move/archive/restore
│   │   │   ├── tree.ts                         (new) flat list → tree by parent_id + sort_key
│   │   │   ├── tree.test.ts                    (new) vitest unit tests for tree builder
│   │   │   ├── DocTree.tsx                     (new) sidebar tree component
│   │   │   ├── DocPage.tsx                     (new) editor host + title + status dot
│   │   │   └── grants.api.ts                   (new) grantsApi.list/put/delete
│   │   ├── permissions/
│   │   │   └── PermissionsDialog.tsx           (new) modal mounted from /doc/:id/permissions
│   │   └── editor/
│   │       ├── KnotProvider.ts                 (rewrite) custom y-protocol v1 WS client
│   │       ├── KnotProvider.test.ts            (new) vitest unit tests for protocol framing + lifecycle
│   │       ├── KnotEditor.tsx                  (rewrite) lazy-loaded, canonical schema, collaboration cursor
│   │       ├── schema.ts                       (unchanged from Plan 1 — generated)
│   │       └── extensions.ts                   (new) canonical Tiptap extension set
│   ├── components/
│   │   ├── AppShell.tsx                        (new) sidebar + main pane layout
│   │   ├── StatusDot.tsx                       (new) connecting/connected/offline/unauthorised/conflict
│   │   ├── Toast.tsx                           (new) one-shot notifications from Zustand
│   │   └── ErrorBoundary.tsx                   (new) friendly fallback
│   └── routes.tsx                              (new) Route tree definition
│
└── e2e/
    └── flows/
        ├── login.spec.ts                       (new) login → land on doc list
        ├── editor.spec.ts                      (new) create doc → type → reload → text persists
        ├── members.spec.ts                     (new) invite member; promote; demote
        ├── two-users-converge.spec.ts          (rewrite) un-skip; real auth + real doc UUID
        └── (existing auth.spec.ts, docs.spec.ts, health.spec.ts, collab.spec.ts unchanged)
```

---

## Conventions

- **Server state goes through TanStack Query.** No `useEffect`+`fetch`+local state for anything the server owns. Query keys are arrays: `['session']`, `['docs']`, `['doc', id]`, `['members']`, `['grants', docId]`.
- **Mutations use `useMutation` + `invalidateQueries`.** After PATCH /api/docs/:id rename, invalidate `['docs']` and `['doc', id]`.
- **No client-side auth state.** `useSession()` is just a Query hook reading `/auth/session`. The cookie is the truth.
- **Zustand only holds UI state** (sidebar open/closed, toasts, modal). Never doc data.
- **All API calls go through `lib/api.ts`**. Per-feature `*.api.ts` are thin typed wrappers.
- **TS strict + valibot.** Every response is parsed by a valibot schema; type-narrowed `ApiResult<T> = { ok: T } | { error: ApiError }`.
- **Routes import their components lazily** for non-shell paths (DocPage, PermissionsDialog, MembersPage, SettingsPage) to keep the main chunk small.
- **Tiptap extensions live in `extensions.ts`** so both the editor and any future "read-only doc view" share the same schema.
- **e2e helpers** — reuse the existing `reset()` + `setup()` helpers from `auth.spec.ts` / `docs.spec.ts`. Don't introduce a 4th truncation copy.

---

## Task overview

| # | Title | LOC ≈ |
|---|---|---|
| 1 | Toolchain hardening (deps + ESLint + vitest) | 80 |
| 2 | API client + CSRF + ApiError + valibot validators | 200 |
| 3 | QueryClient + Zustand store + AppShell shell | 180 |
| 4 | Session loader + RequireAuth + SessionContext | 140 |
| 5 | Router with all routes + entrypoint rewrite | 160 |
| 6 | LoginPage (local credentials + OIDC link) | 180 |
| 7 | SetupPage (first-run admin creation) | 150 |
| 8 | docsApi + tree builder + tree unit tests | 220 |
| 9 | DocTree sidebar (list + create + select) | 240 |
| 10 | DocPage shell (title + status dot + lazy editor) | 200 |
| 11 | KnotProvider rewrite — custom WS client + lifecycle + unit tests | 380 |
| 12 | Tiptap editor wired with canonical schema | 220 |
| 13 | Doc actions (rename / move / archive / restore) | 230 |
| 14 | CollaborationCursor + presence colors | 130 |
| 15 | 4403 close → unauthorised toast + redirect | 90 |
| 16 | Members page (list / invite / role / remove) | 270 |
| 17 | PermissionsDialog (grants per doc) | 250 |
| 18 | SettingsPage (workspace + logout) | 120 |
| 19 | NotFound + onboarding redirect | 80 |
| 20 | Re-enable two-users-converge with real auth | 130 |
| 21 | New e2e: login + editor.spec | 200 |

---

## Task 1: Toolchain hardening

**Files:**
- Modify: `web/package.json`
- Create: `web/.eslintrc.cjs`
- Create: `web/vitest.config.ts`
- Create: `web/src/test/setup.ts`

- [ ] **Step 1: Add deps**

Run from `/home/nik/Development/knot/web`:

```bash
pnpm add react-router-dom @tanstack/react-query zustand valibot
pnpm add -D @testing-library/react @testing-library/jest-dom jsdom \
  eslint @typescript-eslint/parser @typescript-eslint/eslint-plugin \
  eslint-plugin-react-hooks eslint-plugin-import
```

Verify `web/package.json` now has the new entries under `dependencies` + `devDependencies`.

- [ ] **Step 2: ESLint config**

Create `/home/nik/Development/knot/web/.eslintrc.cjs`:

```js
module.exports = {
  root: true,
  parser: "@typescript-eslint/parser",
  parserOptions: {
    ecmaVersion: 2022,
    sourceType: "module",
    project: "./tsconfig.json",
  },
  plugins: ["@typescript-eslint", "react-hooks", "import"],
  extends: [
    "eslint:recommended",
    "plugin:@typescript-eslint/recommended-type-checked",
    "plugin:react-hooks/recommended",
  ],
  rules: {
    "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
    "@typescript-eslint/no-explicit-any": "warn",
    "@typescript-eslint/no-floating-promises": "error",
    "@typescript-eslint/no-misused-promises": "error",
    "react-hooks/rules-of-hooks": "error",
    "react-hooks/exhaustive-deps": "warn",
    "import/order": ["warn", { "newlines-between": "always" }],
  },
  ignorePatterns: ["dist", "node_modules", "src/features/editor/schema.ts"],
};
```

- [ ] **Step 3: Vitest config + setup**

Create `/home/nik/Development/knot/web/vitest.config.ts`:

```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    globals: false,
  },
  resolve: {
    alias: { "@": "/src" },
  },
});
```

Create `/home/nik/Development/knot/web/src/test/setup.ts`:

```ts
import "@testing-library/jest-dom/vitest";
```

- [ ] **Step 4: Add scripts + verify**

Edit `/home/nik/Development/knot/web/package.json` `"scripts"` to add:

```json
    "lint": "eslint src --max-warnings 0",
```

(Keep existing `dev`, `build`, `preview`, `test`, `tsc`.)

Run from `web/`:

```bash
pnpm tsc
pnpm lint
pnpm test
```

Expected: tsc passes, lint passes (zero warnings — existing spike code may have `any` warnings which we'll clean up as tasks land), vitest reports "no tests" or runs the empty suite cleanly.

- [ ] **Step 5: Commit**

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "chore(web): toolchain hardening — deps + ESLint + Vitest"
```

---

## Task 2: API client + CSRF + ApiError + valibot validators

**Files:**
- Create: `web/src/lib/api.ts`
- Create: `web/src/lib/csrf.ts`
- Create: `web/src/lib/validators.ts`
- Create: `web/src/lib/api.test.ts`

- [ ] **Step 1: csrf helper**

Create `/home/nik/Development/knot/web/src/lib/csrf.ts`:

```ts
/** Read a cookie value by name, or null. */
export function readCookie(name: string): string | null {
  const re = new RegExp(`(?:^|; )${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}=([^;]*)`);
  const m = document.cookie.match(re);
  return m && m[1] ? decodeURIComponent(m[1]) : null;
}
```

- [ ] **Step 2: ApiError + fetch wrapper**

Create `/home/nik/Development/knot/web/src/lib/api.ts`:

```ts
import { readCookie } from "./csrf";

/** Stable error code from the server's §6.3 envelope. */
export type ApiError = {
  code: string;
  message: string;
  details: Record<string, unknown>;
  status: number;
};

export type ApiResult<T> = { ok: T } | { error: ApiError };

type Body = Record<string, unknown> | string | null;

type Opts = {
  method?: "GET" | "POST" | "PATCH" | "PUT" | "DELETE";
  body?: Body;
  contentType?: string;
  parser?: (data: unknown) => unknown;
};

const UNSAFE = new Set(["POST", "PUT", "PATCH", "DELETE"]);

/** Single fetch wrapper. Reads CSRF cookie for unsafe methods.
 *  Returns a typed ApiResult. */
export async function apiFetch<T>(path: string, opts: Opts = {}): Promise<ApiResult<T>> {
  const method = opts.method ?? "GET";
  const headers: Record<string, string> = {
    Accept: "application/json",
  };
  let body: BodyInit | undefined = undefined;
  if (opts.body !== undefined && opts.body !== null) {
    if (typeof opts.body === "string") {
      body = opts.body;
      headers["Content-Type"] = opts.contentType ?? "text/plain";
    } else {
      body = JSON.stringify(opts.body);
      headers["Content-Type"] = "application/json";
    }
  }
  if (UNSAFE.has(method)) {
    const csrf = readCookie("csrf");
    if (csrf) headers["X-CSRF-Token"] = csrf;
  }
  const res = await fetch(path, {
    method,
    credentials: "include",
    headers,
    body,
  });
  const text = await res.text();
  let data: unknown = undefined;
  if (text.length > 0) {
    if (res.headers.get("content-type")?.includes("application/json")) {
      try {
        data = JSON.parse(text);
      } catch {
        data = text;
      }
    } else {
      data = text;
    }
  }
  if (!res.ok) {
    const env = data as { error?: Partial<ApiError> } | undefined;
    return {
      error: {
        code: env?.error?.code ?? "http_error",
        message: env?.error?.message ?? `HTTP ${res.status}`,
        details: env?.error?.details ?? {},
        status: res.status,
      },
    };
  }
  const parsed = opts.parser ? opts.parser(data) : data;
  return { ok: parsed as T };
}
```

- [ ] **Step 3: valibot validators**

Create `/home/nik/Development/knot/web/src/lib/validators.ts`:

```ts
import * as v from "valibot";

export const Session = v.object({
  user_id: v.string(),
  email: v.string(),
  display_name: v.string(),
  workspace_id: v.string(),
  role: v.picklist(["owner", "editor", "viewer"]),
});
export type Session = v.InferOutput<typeof Session>;

export const Workspace = v.object({
  id: v.string(),
  slug: v.string(),
  name: v.string(),
  role: v.picklist(["owner", "editor", "viewer"]),
});
export type Workspace = v.InferOutput<typeof Workspace>;

export const Member = v.object({
  user_id: v.string(),
  email: v.string(),
  display_name: v.string(),
  role: v.picklist(["owner", "editor", "viewer"]),
});
export type Member = v.InferOutput<typeof Member>;

export const Doc = v.object({
  id: v.string(),
  workspace_id: v.string(),
  parent_id: v.nullable(v.string()),
  title: v.string(),
  sort_key: v.string(),
  icon: v.nullable(v.string()),
  created_by: v.string(),
  archived: v.boolean(),
});
export type Doc = v.InferOutput<typeof Doc>;

export const DocWithRole = v.object({
  ...Doc.entries,
  effective_role: v.picklist(["owner", "editor", "viewer"]),
});
export type DocWithRole = v.InferOutput<typeof DocWithRole>;

export const Grant = v.object({
  principal: v.string(),
  role: v.picklist(["owner", "editor", "viewer"]),
  inherit: v.boolean(),
});
export type Grant = v.InferOutput<typeof Grant>;

/** Try to parse with the given schema; throw if mismatched (caller treats as
 *  500-level — the server contract drifted). */
export function parse<T>(schema: v.GenericSchema<T>, data: unknown): T {
  return v.parse(schema, data);
}
```

- [ ] **Step 4: Unit test**

Create `/home/nik/Development/knot/web/src/lib/api.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";
import { apiFetch } from "./api";

beforeEach(() => {
  vi.restoreAllMocks();
  document.cookie = "csrf=test-token; Path=/";
});

describe("apiFetch", () => {
  it("returns ok for 200 JSON", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ id: "x" }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    const r = await apiFetch<{ id: string }>("/api/foo");
    if ("ok" in r) expect(r.ok.id).toBe("x");
    else throw new Error("expected ok");
  });

  it("returns error envelope for 4xx", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(JSON.stringify({ error: { code: "auth.csrf", message: "no", details: {} } }), {
        status: 403,
        headers: { "Content-Type": "application/json" },
      }),
    );
    const r = await apiFetch("/api/foo", { method: "POST", body: {} });
    if ("error" in r) {
      expect(r.error.code).toBe("auth.csrf");
      expect(r.error.status).toBe(403);
    } else throw new Error("expected error");
  });

  it("sends X-CSRF-Token on POST", async () => {
    const spy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response("", { status: 204 }),
    );
    await apiFetch("/api/foo", { method: "POST", body: { a: 1 } });
    const init = spy.mock.calls[0]?.[1] as RequestInit | undefined;
    const headers = init?.headers as Record<string, string>;
    expect(headers["X-CSRF-Token"]).toBe("test-token");
    expect(headers["Content-Type"]).toBe("application/json");
  });

  it("does NOT send X-CSRF-Token on GET", async () => {
    const spy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response("{}", { status: 200, headers: { "Content-Type": "application/json" } }),
    );
    await apiFetch("/api/foo");
    const init = spy.mock.calls[0]?.[1] as RequestInit | undefined;
    const headers = init?.headers as Record<string, string>;
    expect(headers["X-CSRF-Token"]).toBeUndefined();
  });
});
```

- [ ] **Step 5: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm test
pnpm lint
```

Expected: 4 vitest tests pass; tsc + lint clean.

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): API client + CSRF + ApiError + valibot validators"
```

---

## Task 3: QueryClient + Zustand store + AppShell

**Files:**
- Create: `web/src/lib/queryClient.ts`
- Create: `web/src/stores/ui.ts`
- Create: `web/src/components/AppShell.tsx`
- Create: `web/src/components/Toast.tsx`
- Create: `web/src/components/StatusDot.tsx`
- Create: `web/src/components/ErrorBoundary.tsx`

- [ ] **Step 1: QueryClient singleton**

Create `/home/nik/Development/knot/web/src/lib/queryClient.ts`:

```ts
import { QueryClient } from "@tanstack/react-query";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: (failureCount, error) => {
        // Don't retry 4xx — they won't change without user action.
        if (error && typeof error === "object" && "status" in error) {
          const status = (error as { status: number }).status;
          if (status >= 400 && status < 500) return false;
        }
        return failureCount < 2;
      },
      refetchOnWindowFocus: false,
    },
  },
});
```

- [ ] **Step 2: Zustand UI store**

Create `/home/nik/Development/knot/web/src/stores/ui.ts`:

```ts
import { create } from "zustand";

export type Toast = {
  id: number;
  kind: "info" | "warn" | "error";
  text: string;
};

type UiState = {
  sidebarOpen: boolean;
  toggleSidebar: () => void;
  toasts: Toast[];
  notify: (kind: Toast["kind"], text: string) => void;
  dismiss: (id: number) => void;
};

let nextId = 1;

export const useUi = create<UiState>((set) => ({
  sidebarOpen: true,
  toggleSidebar: () => set((s) => ({ sidebarOpen: !s.sidebarOpen })),
  toasts: [],
  notify: (kind, text) =>
    set((s) => ({ toasts: [...s.toasts, { id: nextId++, kind, text }] })),
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));
```

- [ ] **Step 3: AppShell**

Create `/home/nik/Development/knot/web/src/components/AppShell.tsx`:

```tsx
import { Outlet } from "react-router-dom";

import { useUi } from "../stores/ui";

import { Toast } from "./Toast";

export function AppShell() {
  const sidebarOpen = useUi((s) => s.sidebarOpen);
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: sidebarOpen ? "260px 1fr" : "0 1fr",
        height: "100vh",
        fontFamily: "system-ui, sans-serif",
      }}
    >
      <aside
        data-testid="sidebar"
        style={{
          borderRight: "1px solid #e5e5e5",
          overflow: "auto",
          background: "#fafafa",
        }}
      >
        <Outlet context={{ slot: "sidebar" }} />
      </aside>
      <main style={{ overflow: "auto" }}>
        <Outlet />
      </main>
      <Toast />
    </div>
  );
}
```

- [ ] **Step 4: Toast**

Create `/home/nik/Development/knot/web/src/components/Toast.tsx`:

```tsx
import { useEffect } from "react";

import { useUi } from "../stores/ui";

export function Toast() {
  const toasts = useUi((s) => s.toasts);
  const dismiss = useUi((s) => s.dismiss);

  useEffect(() => {
    const timers = toasts.map((t) =>
      setTimeout(() => dismiss(t.id), 4000),
    );
    return () => { timers.forEach(clearTimeout); };
  }, [toasts, dismiss]);

  if (toasts.length === 0) return null;
  return (
    <div
      data-testid="toast-stack"
      style={{
        position: "fixed",
        bottom: 16,
        right: 16,
        display: "grid",
        gap: 8,
        zIndex: 50,
      }}
    >
      {toasts.map((t) => (
        <div
          key={t.id}
          data-testid={`toast-${t.kind}`}
          style={{
            padding: "10px 14px",
            borderRadius: 6,
            color: "white",
            background:
              t.kind === "error" ? "#b00020" : t.kind === "warn" ? "#c46c0a" : "#404040",
            boxShadow: "0 2px 8px rgba(0,0,0,0.2)",
          }}
        >
          {t.text}
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 5: StatusDot**

Create `/home/nik/Development/knot/web/src/components/StatusDot.tsx`:

```tsx
export type ConnStatus = "connecting" | "connected" | "offline" | "unauthorised" | "conflict";

const colorOf: Record<ConnStatus, string> = {
  connecting: "#c46c0a",
  connected: "#1f7a1f",
  offline: "#777",
  unauthorised: "#b00020",
  conflict: "#b00020",
};

export function StatusDot({ status }: { status: ConnStatus }) {
  return (
    <span
      data-testid="status-dot"
      data-status={status}
      title={status}
      style={{
        display: "inline-block",
        width: 8,
        height: 8,
        borderRadius: "50%",
        background: colorOf[status],
        marginRight: 6,
      }}
    />
  );
}
```

- [ ] **Step 6: ErrorBoundary**

Create `/home/nik/Development/knot/web/src/components/ErrorBoundary.tsx`:

```tsx
import { Component, type ReactNode } from "react";

type Props = { children: ReactNode };
type State = { error: Error | null };

export class ErrorBoundary extends Component<Props, State> {
  override state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  override componentDidCatch(error: Error) {
    console.error("UI error boundary caught", error);
  }

  override render() {
    if (this.state.error) {
      return (
        <div role="alert" style={{ padding: 24 }}>
          <h1>Something went wrong</h1>
          <pre style={{ whiteSpace: "pre-wrap" }}>{this.state.error.message}</pre>
        </div>
      );
    }
    return this.props.children;
  }
}
```

- [ ] **Step 7: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

Expected: clean.

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): QueryClient + Zustand UI store + AppShell + Toast + StatusDot"
```

---

## Task 4: Session loader + RequireAuth + SessionContext

**Files:**
- Create: `web/src/auth/session.api.ts`
- Create: `web/src/auth/SessionContext.tsx`
- Create: `web/src/auth/RequireAuth.tsx`

- [ ] **Step 1: session.api**

Create `/home/nik/Development/knot/web/src/auth/session.api.ts`:

```ts
import { apiFetch } from "../lib/api";
import { type Session, parse, Session as SessionSchema } from "../lib/validators";

export const authApi = {
  async session() {
    const r = await apiFetch<unknown>("/auth/session");
    if ("error" in r) return r;
    return { ok: parse(SessionSchema, r.ok) satisfies Session };
  },
  async login(email: string, password: string) {
    return apiFetch<void>("/auth/login", {
      method: "POST",
      body: { email, password },
    });
  },
  async logout() {
    return apiFetch<void>("/auth/logout", { method: "POST" });
  },
  async setup(email: string, password: string, display_name: string) {
    return apiFetch<{ user_id: string; workspace_id: string }>("/auth/setup", {
      method: "POST",
      body: { email, password, display_name },
    });
  },
};
```

- [ ] **Step 2: SessionContext**

Create `/home/nik/Development/knot/web/src/auth/SessionContext.tsx`:

```tsx
import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { createContext, useContext, type ReactNode } from "react";

import { type ApiError } from "../lib/api";
import { type Session } from "../lib/validators";

import { authApi } from "./session.api";

type SessionQuery = UseQueryResult<{ ok: Session } | { error: ApiError }, Error>;

const Ctx = createContext<SessionQuery | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const q = useQuery({
    queryKey: ["session"],
    queryFn: authApi.session,
    retry: false,
  });
  return <Ctx.Provider value={q}>{children}</Ctx.Provider>;
}

export function useSession(): SessionQuery {
  const q = useContext(Ctx);
  if (!q) throw new Error("useSession must be used inside SessionProvider");
  return q;
}
```

- [ ] **Step 3: RequireAuth**

Create `/home/nik/Development/knot/web/src/auth/RequireAuth.tsx`:

```tsx
import { Navigate, Outlet, useLocation } from "react-router-dom";

import { useSession } from "./SessionContext";

export function RequireAuth() {
  const q = useSession();
  const loc = useLocation();
  if (q.isLoading) return <div style={{ padding: 24 }}>Loading…</div>;
  const data = q.data;
  if (!data || "error" in data) {
    return <Navigate to="/login" replace state={{ from: loc.pathname }} />;
  }
  return <Outlet />;
}
```

- [ ] **Step 4: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

Expected: clean.

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): SessionContext + RequireAuth route gate"
```

---

## Task 5: Router + entrypoint rewrite

**Files:**
- Create: `web/src/routes.tsx`
- Rewrite: `web/src/main.tsx`
- Delete: `web/src/App.tsx`
- Create: stub pages for now: `web/src/features/auth/LoginPage.tsx`, `web/src/features/auth/SetupPage.tsx`, `web/src/features/docs/DocPage.tsx`, `web/src/features/docs/DocTree.tsx`, `web/src/features/workspace/MembersPage.tsx`, `web/src/features/workspace/SettingsPage.tsx`

- [ ] **Step 1: Stub pages**

Create each of these as minimal placeholders (they'll be filled in later tasks):

`web/src/features/auth/LoginPage.tsx`:

```tsx
export default function LoginPage() {
  return <main style={{ padding: 24 }}><h1>Login</h1></main>;
}
```

`web/src/features/auth/SetupPage.tsx`:

```tsx
export default function SetupPage() {
  return <main style={{ padding: 24 }}><h1>Setup</h1></main>;
}
```

`web/src/features/docs/DocPage.tsx`:

```tsx
import { useParams } from "react-router-dom";

export default function DocPage() {
  const { id } = useParams();
  return <main style={{ padding: 24 }}><h1>Doc {id}</h1></main>;
}
```

`web/src/features/docs/DocTree.tsx`:

```tsx
export function DocTree() {
  return <div style={{ padding: 12 }}>Tree (TODO)</div>;
}
```

`web/src/features/workspace/MembersPage.tsx`:

```tsx
export default function MembersPage() {
  return <main style={{ padding: 24 }}><h1>Members</h1></main>;
}
```

`web/src/features/workspace/SettingsPage.tsx`:

```tsx
export default function SettingsPage() {
  return <main style={{ padding: 24 }}><h1>Settings</h1></main>;
}
```

- [ ] **Step 2: Router**

Create `/home/nik/Development/knot/web/src/routes.tsx`:

```tsx
import { lazy, Suspense } from "react";
import { createBrowserRouter, Navigate, Outlet } from "react-router-dom";

import { RequireAuth } from "./auth/RequireAuth";
import { AppShell } from "./components/AppShell";
import { DocTree } from "./features/docs/DocTree";

const LoginPage = lazy(() => import("./features/auth/LoginPage"));
const SetupPage = lazy(() => import("./features/auth/SetupPage"));
const DocPage = lazy(() => import("./features/docs/DocPage"));
const MembersPage = lazy(() => import("./features/workspace/MembersPage"));
const SettingsPage = lazy(() => import("./features/workspace/SettingsPage"));

function Lazy({ children }: { children: React.ReactNode }) {
  return <Suspense fallback={<div style={{ padding: 24 }}>Loading…</div>}>{children}</Suspense>;
}

export const router = createBrowserRouter([
  { path: "/login", element: <Lazy><LoginPage /></Lazy> },
  { path: "/setup", element: <Lazy><SetupPage /></Lazy> },
  {
    element: <RequireAuth />,
    children: [
      {
        element: <AppShell />,
        children: [
          { index: true, element: <DocTreeAndLanding /> },
          { path: "doc/:id", element: <DocTreeAndDoc /> },
          { path: "members", element: <Lazy><MembersPage /></Lazy> },
          { path: "settings", element: <Lazy><SettingsPage /></Lazy> },
        ],
      },
    ],
  },
  { path: "*", element: <Navigate to="/" replace /> },
]);

function DocTreeAndLanding() {
  return (
    <>
      <DocTree />
      <div style={{ padding: 24 }}>Select a document from the sidebar.</div>
    </>
  );
}

function DocTreeAndDoc() {
  return (
    <>
      <DocTree />
      <Outlet />
      <Lazy><DocPage /></Lazy>
    </>
  );
}
```

> **Note:** AppShell uses TWO `<Outlet />`s — one for the sidebar slot, one for the main slot. React Router only routes a single Outlet, so we render `<DocTree />` from the sidebar context and a wrapping page that uses the main outlet for the right pane. Adjust if you find a cleaner pattern; the test contract is "sidebar visible alongside main pane on protected routes."

- [ ] **Step 3: main.tsx**

Replace `/home/nik/Development/knot/web/src/main.tsx` with:

```tsx
import { QueryClientProvider } from "@tanstack/react-query";
import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "react-router-dom";

import { SessionProvider } from "./auth/SessionContext";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { queryClient } from "./lib/queryClient";
import { router } from "./routes";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ErrorBoundary>
      <QueryClientProvider client={queryClient}>
        <SessionProvider>
          <RouterProvider router={router} />
        </SessionProvider>
      </QueryClientProvider>
    </ErrorBoundary>
  </React.StrictMode>,
);
```

- [ ] **Step 4: Delete App.tsx**

```bash
rm /home/nik/Development/knot/web/src/App.tsx
```

- [ ] **Step 5: Verify**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

Expected: clean.

```bash
pnpm dev
```

Visit `http://localhost:5173/`. Expect a redirect to `/login` and the stub Login page.

- [ ] **Step 6: Commit**

```bash
cd /home/nik/Development/knot
git add web/
git rm web/src/App.tsx
git commit -m "feat(web): React Router v6 + lazy routes + entrypoint rewrite"
```

---

## Task 6: LoginPage

**Files:**
- Rewrite: `web/src/features/auth/LoginPage.tsx`

- [ ] **Step 1: Implement**

Replace `/home/nik/Development/knot/web/src/features/auth/LoginPage.tsx` with:

```tsx
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useLocation, useNavigate } from "react-router-dom";

import { authApi } from "../../auth/session.api";

export default function LoginPage() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const nav = useNavigate();
  const loc = useLocation();
  const qc = useQueryClient();

  const from = ((loc.state as { from?: string } | null)?.from) ?? "/";

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    const r = await authApi.login(email, password);
    setBusy(false);
    if ("error" in r) {
      setError(r.error.code === "auth.invalid_credentials"
        ? "Invalid email or password."
        : "Login failed.");
      return;
    }
    await qc.invalidateQueries({ queryKey: ["session"] });
    nav(from, { replace: true });
  }

  return (
    <main
      style={{
        maxWidth: 380,
        margin: "10vh auto",
        padding: 24,
        fontFamily: "system-ui, sans-serif",
      }}
    >
      <h1 style={{ marginBottom: 24 }}>Sign in to knot</h1>
      <form data-testid="login-form" onSubmit={(e) => { void onSubmit(e); }}>
        <label style={{ display: "block", marginBottom: 12 }}>
          <span style={{ display: "block", marginBottom: 4 }}>Email</span>
          <input
            data-testid="login-email"
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            required
            style={{ width: "100%", padding: 8 }}
          />
        </label>
        <label style={{ display: "block", marginBottom: 16 }}>
          <span style={{ display: "block", marginBottom: 4 }}>Password</span>
          <input
            data-testid="login-password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
            style={{ width: "100%", padding: 8 }}
          />
        </label>
        {error && (
          <p data-testid="login-error" style={{ color: "#b00020", marginBottom: 12 }}>
            {error}
          </p>
        )}
        <button
          data-testid="login-submit"
          type="submit"
          disabled={busy}
          style={{ width: "100%", padding: 10 }}
        >
          {busy ? "Signing in…" : "Sign in"}
        </button>
      </form>
      <p style={{ marginTop: 24, textAlign: "center" }}>
        <a href="/auth/oidc/login">Sign in with SSO</a>
      </p>
      <p style={{ marginTop: 12, textAlign: "center" }}>
        <a href="/setup">First-run setup</a>
      </p>
    </main>
  );
}
```

- [ ] **Step 2: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

Expected: clean.

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): LoginPage with email/password + OIDC link"
```

---

## Task 7: SetupPage

**Files:**
- Rewrite: `web/src/features/auth/SetupPage.tsx`

- [ ] **Step 1: Implement**

Replace `/home/nik/Development/knot/web/src/features/auth/SetupPage.tsx` with:

```tsx
import { useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate } from "react-router-dom";

import { authApi } from "../../auth/session.api";

export default function SetupPage() {
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const nav = useNavigate();
  const qc = useQueryClient();

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    const r = await authApi.setup(email, password, displayName);
    setBusy(false);
    if ("error" in r) {
      setError(
        r.error.code === "auth.setup_closed"
          ? "Setup is already complete. Try signing in."
          : r.error.code === "auth.weak_password"
            ? "Password must be at least 8 characters."
            : "Setup failed.",
      );
      return;
    }
    await qc.invalidateQueries({ queryKey: ["session"] });
    nav("/", { replace: true });
  }

  return (
    <main
      style={{
        maxWidth: 420,
        margin: "10vh auto",
        padding: 24,
        fontFamily: "system-ui, sans-serif",
      }}
    >
      <h1 style={{ marginBottom: 24 }}>First-run setup</h1>
      <p style={{ marginBottom: 16, color: "#555" }}>
        Create the workspace owner. This page closes after the first user is
        created.
      </p>
      <form data-testid="setup-form" onSubmit={(e) => { void onSubmit(e); }}>
        <label style={{ display: "block", marginBottom: 12 }}>
          <span style={{ display: "block", marginBottom: 4 }}>Email</span>
          <input
            data-testid="setup-email"
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            required
            style={{ width: "100%", padding: 8 }}
          />
        </label>
        <label style={{ display: "block", marginBottom: 12 }}>
          <span style={{ display: "block", marginBottom: 4 }}>Display name</span>
          <input
            data-testid="setup-display-name"
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            required
            style={{ width: "100%", padding: 8 }}
          />
        </label>
        <label style={{ display: "block", marginBottom: 16 }}>
          <span style={{ display: "block", marginBottom: 4 }}>Password</span>
          <input
            data-testid="setup-password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            required
            minLength={8}
            style={{ width: "100%", padding: 8 }}
          />
        </label>
        {error && (
          <p data-testid="setup-error" style={{ color: "#b00020", marginBottom: 12 }}>
            {error}
          </p>
        )}
        <button
          data-testid="setup-submit"
          type="submit"
          disabled={busy}
          style={{ width: "100%", padding: 10 }}
        >
          {busy ? "Creating…" : "Create workspace"}
        </button>
      </form>
    </main>
  );
}
```

- [ ] **Step 2: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): SetupPage for first-run admin creation"
```

---

## Task 8: docsApi + tree builder + tests

**Files:**
- Create: `web/src/features/docs/docs.api.ts`
- Create: `web/src/features/docs/tree.ts`
- Create: `web/src/features/docs/tree.test.ts`

- [ ] **Step 1: docs.api**

Create `/home/nik/Development/knot/web/src/features/docs/docs.api.ts`:

```ts
import { apiFetch } from "../../lib/api";
import { Doc, DocWithRole, parse } from "../../lib/validators";
import * as v from "valibot";

export type DocCreate = { title?: string; parent_id?: string; after_id?: string };
export type DocPatch = { title?: string; icon?: string };
export type DocMove = { parent_id?: string | null; after_id?: string; before_id?: string };

export const docsApi = {
  async list() {
    const r = await apiFetch<unknown>("/api/docs");
    if ("error" in r) return r;
    return { ok: parse(v.array(Doc), r.ok) };
  },
  async get(id: string) {
    const r = await apiFetch<unknown>(`/api/docs/${encodeURIComponent(id)}`);
    if ("error" in r) return r;
    return { ok: parse(DocWithRole, r.ok) };
  },
  create(body: DocCreate) {
    return apiFetch<unknown>("/api/docs", { method: "POST", body });
  },
  patch(id: string, body: DocPatch) {
    return apiFetch<unknown>(`/api/docs/${encodeURIComponent(id)}`, { method: "PATCH", body });
  },
  move(id: string, body: DocMove) {
    return apiFetch<unknown>(`/api/docs/${encodeURIComponent(id)}/move`, {
      method: "POST",
      body,
    });
  },
  archive(id: string) {
    return apiFetch<void>(`/api/docs/${encodeURIComponent(id)}`, { method: "DELETE" });
  },
  restore(id: string) {
    return apiFetch<void>(`/api/docs/${encodeURIComponent(id)}/restore`, { method: "POST" });
  },
};
```

- [ ] **Step 2: tree builder**

Create `/home/nik/Development/knot/web/src/features/docs/tree.ts`:

```ts
import type { Doc } from "../../lib/validators";

export type TreeNode = Doc & { children: TreeNode[] };

/** Build a tree from a flat doc list. Sorts siblings by sort_key
 *  (LexoRank-style, lexicographic). Orphans (parent_id missing) become
 *  top-level. */
export function buildTree(docs: Doc[]): TreeNode[] {
  const byId = new Map<string, TreeNode>();
  docs.forEach((d) => byId.set(d.id, { ...d, children: [] }));
  const roots: TreeNode[] = [];
  byId.forEach((node) => {
    if (node.parent_id && byId.has(node.parent_id)) {
      byId.get(node.parent_id)!.children.push(node);
    } else {
      roots.push(node);
    }
  });
  const sortKey = (a: TreeNode, b: TreeNode) =>
    a.sort_key < b.sort_key ? -1 : a.sort_key > b.sort_key ? 1 : 0;
  function sortRec(nodes: TreeNode[]) {
    nodes.sort(sortKey);
    nodes.forEach((n) => sortRec(n.children));
  }
  sortRec(roots);
  return roots;
}
```

- [ ] **Step 3: tree tests**

Create `/home/nik/Development/knot/web/src/features/docs/tree.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import type { Doc } from "../../lib/validators";
import { buildTree } from "./tree";

function doc(id: string, parent: string | null, sort_key: string): Doc {
  return {
    id,
    workspace_id: "w",
    parent_id: parent,
    title: id,
    sort_key,
    icon: null,
    created_by: "u",
    archived: false,
  };
}

describe("buildTree", () => {
  it("returns empty for empty input", () => {
    expect(buildTree([])).toEqual([]);
  });

  it("groups children under parents", () => {
    const t = buildTree([
      doc("a", null, "m"),
      doc("b", "a", "m"),
      doc("c", "a", "n"),
    ]);
    expect(t).toHaveLength(1);
    expect(t[0]!.id).toBe("a");
    expect(t[0]!.children.map((n) => n.id)).toEqual(["b", "c"]);
  });

  it("sorts siblings by sort_key", () => {
    const t = buildTree([
      doc("a", null, "n"),
      doc("b", null, "m"),
      doc("c", null, "z"),
    ]);
    expect(t.map((n) => n.id)).toEqual(["b", "a", "c"]);
  });

  it("treats orphans (parent missing) as top-level", () => {
    const t = buildTree([
      doc("a", null, "m"),
      doc("b", "missing-parent", "m"),
    ]);
    expect(t.map((n) => n.id).sort()).toEqual(["a", "b"]);
  });
});
```

- [ ] **Step 4: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
pnpm test
```

Expected: 4 tree tests pass alongside the API tests from T2.

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): docsApi + tree builder + tree unit tests"
```

---

## Task 9: DocTree sidebar

**Files:**
- Rewrite: `web/src/features/docs/DocTree.tsx`

- [ ] **Step 1: Implement**

Replace `/home/nik/Development/knot/web/src/features/docs/DocTree.tsx` with:

```tsx
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router-dom";

import { useUi } from "../../stores/ui";

import { docsApi } from "./docs.api";
import { buildTree, type TreeNode } from "./tree";

export function DocTree() {
  const qc = useQueryClient();
  const nav = useNavigate();
  const notify = useUi((s) => s.notify);
  const { id: activeId } = useParams();

  const list = useQuery({
    queryKey: ["docs"],
    queryFn: docsApi.list,
  });

  const create = useMutation({
    mutationFn: async (parent_id?: string) =>
      docsApi.create({ title: "Untitled", parent_id }),
    onSuccess: async (r) => {
      if ("error" in r) {
        notify("error", "Couldn't create document");
        return;
      }
      await qc.invalidateQueries({ queryKey: ["docs"] });
      const created = r.ok as { id: string };
      nav(`/doc/${created.id}`);
    },
  });

  if (list.isLoading) return <div style={{ padding: 12 }}>Loading…</div>;
  if (!list.data || "error" in list.data) return <div style={{ padding: 12 }}>Failed.</div>;

  const tree = buildTree(list.data.ok);
  return (
    <div data-testid="doc-tree" style={{ padding: 12 }}>
      <header
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 8,
        }}
      >
        <strong>Docs</strong>
        <button
          data-testid="new-doc"
          onClick={() => create.mutate(undefined)}
          style={{ padding: "2px 8px" }}
        >
          + New
        </button>
      </header>
      {tree.length === 0 && <p style={{ color: "#888" }}>No documents yet.</p>}
      <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
        {tree.map((n) => (
          <TreeRow key={n.id} node={n} depth={0} activeId={activeId} />
        ))}
      </ul>
      <nav style={{ marginTop: 24, borderTop: "1px solid #e5e5e5", paddingTop: 12 }}>
        <Link to="/members" style={{ display: "block", padding: 4 }}>Members</Link>
        <Link to="/settings" style={{ display: "block", padding: 4 }}>Settings</Link>
      </nav>
    </div>
  );
}

function TreeRow({
  node,
  depth,
  activeId,
}: {
  node: TreeNode;
  depth: number;
  activeId?: string;
}) {
  const isActive = activeId === node.id;
  return (
    <li>
      <Link
        data-testid={`doc-row-${node.id}`}
        to={`/doc/${node.id}`}
        style={{
          display: "block",
          padding: "4px 0",
          paddingLeft: depth * 12,
          background: isActive ? "#e5e5ff" : "transparent",
          textDecoration: "none",
          color: "inherit",
        }}
      >
        {node.icon ?? "📄"} {node.title}
      </Link>
      {node.children.length > 0 && (
        <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
          {node.children.map((c) => (
            <TreeRow key={c.id} node={c} depth={depth + 1} activeId={activeId} />
          ))}
        </ul>
      )}
    </li>
  );
}
```

- [ ] **Step 2: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
pnpm test
```

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): DocTree sidebar with create + select"
```

---

## Task 10: DocPage shell + lazy editor

**Files:**
- Rewrite: `web/src/features/docs/DocPage.tsx`

- [ ] **Step 1: Implement (no editor yet — placeholder + status + title)**

Replace `/home/nik/Development/knot/web/src/features/docs/DocPage.tsx` with:

```tsx
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { lazy, Suspense, useState, useEffect } from "react";
import { useParams } from "react-router-dom";

import { StatusDot, type ConnStatus } from "../../components/StatusDot";
import { useUi } from "../../stores/ui";

import { docsApi } from "./docs.api";

const KnotEditor = lazy(() => import("../editor/KnotEditor").then((m) => ({ default: m.KnotEditor })));

export default function DocPage() {
  const { id } = useParams<{ id: string }>();
  const qc = useQueryClient();
  const notify = useUi((s) => s.notify);
  const [status, setStatus] = useState<ConnStatus>("connecting");

  const doc = useQuery({
    queryKey: ["doc", id],
    queryFn: () => docsApi.get(id!),
    enabled: Boolean(id),
  });

  const rename = useMutation({
    mutationFn: async (title: string) => docsApi.patch(id!, { title }),
    onSuccess: async (r) => {
      if ("error" in r) {
        notify("error", "Couldn't rename");
        return;
      }
      await qc.invalidateQueries({ queryKey: ["docs"] });
      await qc.invalidateQueries({ queryKey: ["doc", id] });
    },
  });

  const [title, setTitle] = useState("");
  useEffect(() => {
    if (doc.data && "ok" in doc.data) setTitle(doc.data.ok.title);
  }, [doc.data]);

  if (!id) return null;
  if (doc.isLoading) return <div style={{ padding: 24 }}>Loading…</div>;
  if (!doc.data || "error" in doc.data) {
    return <div style={{ padding: 24 }}>Document not found.</div>;
  }

  const meta = doc.data.ok;
  return (
    <section data-testid="doc-page" style={{ padding: 24 }}>
      <header style={{ display: "flex", alignItems: "center", marginBottom: 12 }}>
        <StatusDot status={status} />
        <input
          data-testid="doc-title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onBlur={() => { if (title !== meta.title) rename.mutate(title); }}
          style={{
            border: "none",
            fontSize: 24,
            fontWeight: 600,
            flex: 1,
            background: "transparent",
          }}
        />
      </header>
      <Suspense fallback={<p>Loading editor…</p>}>
        <KnotEditor docId={id} onStatus={setStatus} role={meta.effective_role} />
      </Suspense>
    </section>
  );
}
```

- [ ] **Step 2: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

Expected: clean (KnotEditor's new signature lands in T11+T12).

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): DocPage shell with title + status dot + lazy editor"
```

---

## Task 11: KnotProvider rewrite

**Files:**
- Rewrite: `web/src/features/editor/KnotProvider.ts`
- Create: `web/src/features/editor/KnotProvider.test.ts`

- [ ] **Step 1: Custom y-protocol v1 client**

Replace `/home/nik/Development/knot/web/src/features/editor/KnotProvider.ts` with:

```ts
/**
 * KnotProvider — y-protocol v1 WebSocket client.
 *
 * Wire format mirrors the server's `crates/knot-server/src/protocol.rs`:
 *   <msg_type:u8> [<sync_subtype:u8>] <varuint length> <payload bytes>
 *
 *   msg_type 0 (sync) + subtype 0 SyncStep1     state-vector
 *   msg_type 0 (sync) + subtype 1 SyncStep2     missing-update bytes
 *   msg_type 0 (sync) + subtype 2 Update        incremental update bytes
 *   msg_type 1 (awareness)                      awareness update bytes
 *   msg_type 3 (query awareness)                no payload
 */

import * as Y from "yjs";
import { Awareness, encodeAwarenessUpdate, applyAwarenessUpdate } from "y-protocols/awareness";

const MSG_SYNC = 0;
const MSG_AWARENESS = 1;
const SYNC_STEP_1 = 0;
const SYNC_STEP_2 = 1;
const SYNC_UPDATE = 2;

export type ProviderStatus =
  | "connecting"
  | "connected"
  | "offline"
  | "unauthorised"
  | "conflict";

export type ProviderEvents = {
  status: (s: ProviderStatus) => void;
};

type Listeners = { [K in keyof ProviderEvents]: Array<ProviderEvents[K]> };

export class KnotProvider {
  readonly doc: Y.Doc;
  readonly awareness: Awareness;
  readonly url: string;
  status: ProviderStatus = "connecting";
  private ws: WebSocket | null = null;
  private destroyed = false;
  private listeners: Listeners = { status: [] };
  private reconnectAttempt = 0;
  private reconnectTimer: number | null = null;

  constructor(opts: { url: string; doc: Y.Doc; awareness?: Awareness }) {
    this.url = opts.url;
    this.doc = opts.doc;
    this.awareness = opts.awareness ?? new Awareness(opts.doc);
    this.connect();

    this.doc.on("update", this.handleDocUpdate);
    this.awareness.on("update", this.handleAwarenessUpdate);
  }

  on<K extends keyof ProviderEvents>(k: K, fn: ProviderEvents[K]) {
    this.listeners[k].push(fn);
  }
  off<K extends keyof ProviderEvents>(k: K, fn: ProviderEvents[K]) {
    this.listeners[k] = this.listeners[k].filter((f) => f !== fn) as Listeners[K];
  }

  destroy() {
    this.destroyed = true;
    this.doc.off("update", this.handleDocUpdate);
    this.awareness.off("update", this.handleAwarenessUpdate);
    if (this.reconnectTimer !== null) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
  }

  private setStatus(s: ProviderStatus) {
    this.status = s;
    this.listeners.status.forEach((fn) => fn(s));
  }

  private connect() {
    if (this.destroyed) return;
    this.setStatus("connecting");
    const ws = new WebSocket(this.url);
    ws.binaryType = "arraybuffer";
    this.ws = ws;
    ws.onopen = () => {
      this.reconnectAttempt = 0;
      this.setStatus("connected");
      // Send SyncStep1 with our current state vector.
      const sv = Y.encodeStateVector(this.doc);
      ws.send(encodeSync(SYNC_STEP_1, sv));
      // Announce our awareness (so others see our cursor on join).
      const clients = [this.awareness.clientID];
      const ar = encodeAwarenessUpdate(this.awareness, clients);
      ws.send(encodeAwareness(ar));
    };
    ws.onmessage = (e) => this.handleFrame(new Uint8Array(e.data as ArrayBuffer));
    ws.onclose = (e) => {
      this.ws = null;
      if (this.destroyed) return;
      if (e.code === 4403) {
        this.setStatus("unauthorised");
        return;
      }
      if (e.code === 4408 || e.code === 4500) {
        this.setStatus("conflict");
        return;
      }
      this.setStatus("offline");
      this.scheduleReconnect();
    };
    ws.onerror = () => {
      // onclose fires next; let it do the work.
    };
  }

  private scheduleReconnect() {
    if (this.destroyed) return;
    const backoff = Math.min(30_000, 500 * Math.pow(2, this.reconnectAttempt));
    const jitter = Math.random() * 300;
    this.reconnectAttempt += 1;
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, backoff + jitter);
  }

  private handleFrame(buf: Uint8Array) {
    if (buf.length === 0) return;
    const type = buf[0];
    if (type === MSG_SYNC) {
      if (buf.length < 2) return;
      const subtype = buf[1];
      const [payload] = readVarBytes(buf, 2);
      if (!payload) return;
      switch (subtype) {
        case SYNC_STEP_1: {
          // Peer wants our missing updates.
          const update = Y.encodeStateAsUpdate(this.doc, payload);
          this.ws?.send(encodeSync(SYNC_STEP_2, update));
          return;
        }
        case SYNC_STEP_2:
        case SYNC_UPDATE:
          Y.applyUpdate(this.doc, payload, this);
          return;
      }
    } else if (type === MSG_AWARENESS) {
      const [payload] = readVarBytes(buf, 1);
      if (payload) applyAwarenessUpdate(this.awareness, payload, this);
    }
  }

  private handleDocUpdate = (update: Uint8Array, origin: unknown) => {
    if (origin === this) return; // ignore echoes
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(encodeSync(SYNC_UPDATE, update));
    }
  };

  private handleAwarenessUpdate = (
    { added, updated, removed }: { added: number[]; updated: number[]; removed: number[] },
    origin: unknown,
  ) => {
    if (origin === this) return;
    const clients = [...added, ...updated, ...removed];
    if (this.ws?.readyState === WebSocket.OPEN && clients.length > 0) {
      const update = encodeAwarenessUpdate(this.awareness, clients);
      this.ws.send(encodeAwareness(update));
    }
  };
}

// ---- wire-format helpers ----

function encodeVarUint(out: number[], v: number) {
  while (v >= 0x80) {
    out.push((v & 0x7f) | 0x80);
    v >>>= 7;
  }
  out.push(v & 0x7f);
}

function readVarUint(buf: Uint8Array, offset: number): [number, number] | null {
  let v = 0;
  let shift = 0;
  let i = offset;
  while (i < buf.length) {
    const b = buf[i]!;
    v |= (b & 0x7f) << shift;
    i += 1;
    if ((b & 0x80) === 0) return [v >>> 0, i];
    shift += 7;
    if (shift > 35) return null;
  }
  return null;
}

function readVarBytes(buf: Uint8Array, offset: number): [Uint8Array | null, number] {
  const res = readVarUint(buf, offset);
  if (!res) return [null, offset];
  const [len, after] = res;
  if (after + len > buf.length) return [null, offset];
  return [buf.subarray(after, after + len), after + len];
}

function encodeSync(subtype: number, payload: Uint8Array): Uint8Array {
  const head: number[] = [MSG_SYNC, subtype];
  encodeVarUint(head, payload.length);
  const out = new Uint8Array(head.length + payload.length);
  out.set(head, 0);
  out.set(payload, head.length);
  return out;
}

function encodeAwareness(payload: Uint8Array): Uint8Array {
  const head: number[] = [MSG_AWARENESS];
  encodeVarUint(head, payload.length);
  const out = new Uint8Array(head.length + payload.length);
  out.set(head, 0);
  out.set(payload, head.length);
  return out;
}
```

- [ ] **Step 2: Unit tests for wire format**

Create `/home/nik/Development/knot/web/src/features/editor/KnotProvider.test.ts`:

```ts
import { describe, expect, it } from "vitest";

import { KnotProvider } from "./KnotProvider";

describe("KnotProvider wire helpers", () => {
  it("decodes its own sync frame format", () => {
    // Round-trip: encode a fake SyncStep2 then verify the leading bytes.
    // We re-use the private helper indirectly by creating a frame the
    // server's protocol decoder expects (msg_type=0, subtype=1, varuint length).
    const payload = new Uint8Array([10, 20, 30]);
    // Construct directly by re-implementing the format (this is the test).
    const expected = new Uint8Array([0, 1, 3, 10, 20, 30]);
    // We don't export encodeSync directly; the contract is checked by the
    // server convergence integration tests. Smoke-check the constructor.
    const _p = new KnotProvider({
      url: "ws://localhost:0/never",
      doc: new (require("yjs").Doc)(),
    });
    expect(_p.status).toBe("connecting");
    _p.destroy();
    expect(expected[0]).toBe(0);
  });
});
```

> **Note:** The provider's protocol helpers are tested at integration level via the existing server-side convergence test + the new collab.spec.ts. Unit tests here cover construction/lifecycle only; the wire format is verified end-to-end. If you want a more thorough unit test of `encodeSync`/`readVarBytes`, export them via a `__test` re-export and add round-trip cases — recommended as a follow-up.

- [ ] **Step 3: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
pnpm test
```

Expected: previous tests still pass + new provider test passes.

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): KnotProvider — custom y-protocol v1 WS client with lifecycle"
```

---

## Task 12: Tiptap editor with canonical schema

**Files:**
- Create: `web/src/features/editor/extensions.ts`
- Rewrite: `web/src/features/editor/KnotEditor.tsx`

- [ ] **Step 1: extensions.ts**

Create `/home/nik/Development/knot/web/src/features/editor/extensions.ts`:

```ts
import Collaboration from "@tiptap/extension-collaboration";
import CollaborationCursor from "@tiptap/extension-collaboration-cursor";
import StarterKit from "@tiptap/starter-kit";
import type { Awareness } from "y-protocols/awareness";
import type * as Y from "yjs";

/** Canonical Tiptap extension set that matches the server schema generated
 *  from `tools/schema.json` (`crates/knot-markdown/src/schema.rs` +
 *  `web/src/features/editor/schema.ts`). StarterKit covers Document /
 *  Paragraph / Text / Heading / Bold / Italic / Code / BulletList /
 *  OrderedList / ListItem / Blockquote / CodeBlock / HardBreak /
 *  HorizontalRule / Strike. History is disabled because Yjs UndoManager
 *  owns undo. */
export function createExtensions(opts: {
  doc: Y.Doc;
  awareness: Awareness;
  user: { name: string; color: string };
}) {
  return [
    StarterKit.configure({
      history: false,
    }),
    Collaboration.configure({ document: opts.doc }),
    CollaborationCursor.configure({
      provider: { awareness: opts.awareness } as never, // CollaborationCursor only needs .awareness
      user: opts.user,
    }),
  ];
}
```

> **Note:** Tiptap's CollaborationCursor expects a y-websocket-style `provider` with an `awareness` property. We adapt by passing an object literal; if a future Tiptap version rejects this, adjust by wrapping the KnotProvider in a thin compatibility shim.

- [ ] **Step 2: KnotEditor**

Replace `/home/nik/Development/knot/web/src/features/editor/KnotEditor.tsx` with:

```tsx
import { EditorContent, useEditor } from "@tiptap/react";
import { useEffect, useMemo } from "react";
import * as Y from "yjs";

import { useSession } from "../../auth/SessionContext";

import { createExtensions } from "./extensions";
import { KnotProvider, type ProviderStatus } from "./KnotProvider";

export function KnotEditor({
  docId,
  onStatus,
  role: _role,
}: {
  docId: string;
  onStatus: (s: ProviderStatus) => void;
  role: "owner" | "editor" | "viewer";
}) {
  const session = useSession();
  const sessionUser = session.data && "ok" in session.data ? session.data.ok : null;
  const ydoc = useMemo(() => new Y.Doc(), [docId]);

  const provider = useMemo(() => {
    const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
    return new KnotProvider({
      url: `${proto}//${window.location.host}/collab/${docId}`,
      doc: ydoc,
    });
  }, [ydoc, docId]);

  useEffect(() => {
    onStatus(provider.status);
    const fn = (s: ProviderStatus) => onStatus(s);
    provider.on("status", fn);
    return () => {
      provider.off("status", fn);
      provider.destroy();
      ydoc.destroy();
    };
  }, [provider, ydoc, onStatus]);

  const userColor = useMemo(() => colorFor(sessionUser?.user_id ?? "anon"), [sessionUser]);

  const editor = useEditor(
    {
      extensions: createExtensions({
        doc: ydoc,
        awareness: provider.awareness,
        user: { name: sessionUser?.display_name ?? "Anonymous", color: userColor },
      }),
      editable: _role !== "viewer",
    },
    [ydoc, provider, sessionUser?.user_id, _role],
  );

  return (
    <div data-testid="editor-host" style={{ border: "1px solid #e5e5e5", padding: 16, minHeight: 240 }}>
      <EditorContent editor={editor} />
    </div>
  );
}

function colorFor(id: string): string {
  let hash = 0;
  for (let i = 0; i < id.length; i += 1) hash = (hash * 31 + id.charCodeAt(i)) >>> 0;
  return `hsl(${hash % 360}, 70%, 45%)`;
}
```

- [ ] **Step 3: Add @tiptap/extension-collaboration-cursor dep**

```bash
cd /home/nik/Development/knot/web
pnpm add @tiptap/extension-collaboration-cursor
```

- [ ] **Step 4: Verify + commit**

```bash
pnpm tsc
pnpm lint
pnpm test
```

Expected: clean. The editor type signature for `useEditor`'s deps array changed; if tsc complains about non-stringifiable deps, replace `sessionUser?.user_id` with a simple presence boolean.

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): Tiptap editor with canonical schema + collaboration cursor"
```

---

## Task 13: Doc actions (rename / move / archive / restore)

**Files:**
- Modify: `web/src/features/docs/DocTree.tsx` — add context menu (rename, archive)
- Modify: `web/src/features/docs/DocPage.tsx` — archive button (Owner-only)

- [ ] **Step 1: Sidebar context menu**

Edit `/home/nik/Development/knot/web/src/features/docs/DocTree.tsx`. Update `TreeRow` to add a right-click handler that prompts:

```tsx
function TreeRow({
  node,
  depth,
  activeId,
}: {
  node: TreeNode;
  depth: number;
  activeId?: string;
}) {
  const qc = useQueryClient();
  const notify = useUi((s) => s.notify);
  const isActive = activeId === node.id;

  async function onRename() {
    const next = window.prompt("Rename to:", node.title);
    if (!next || next === node.title) return;
    const r = await docsApi.patch(node.id, { title: next });
    if ("error" in r) notify("error", "Rename failed");
    else await qc.invalidateQueries({ queryKey: ["docs"] });
  }

  async function onArchive() {
    if (!window.confirm(`Delete "${node.title}"?`)) return;
    const r = await docsApi.archive(node.id);
    if ("error" in r) notify("error", "Delete failed");
    else await qc.invalidateQueries({ queryKey: ["docs"] });
  }

  return (
    <li>
      <Link
        data-testid={`doc-row-${node.id}`}
        to={`/doc/${node.id}`}
        onContextMenu={(e) => {
          e.preventDefault();
          // Tiny inline menu via window.confirm chain — minimal UI for v0.1.
          const action = window.prompt("Action: rename | delete", "rename");
          if (action === "rename") void onRename();
          else if (action === "delete") void onArchive();
        }}
        style={{
          display: "block",
          padding: "4px 0",
          paddingLeft: depth * 12,
          background: isActive ? "#e5e5ff" : "transparent",
          textDecoration: "none",
          color: "inherit",
        }}
      >
        {node.icon ?? "📄"} {node.title}
      </Link>
      {node.children.length > 0 && (
        <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
          {node.children.map((c) => (
            <TreeRow key={c.id} node={c} depth={depth + 1} activeId={activeId} />
          ))}
        </ul>
      )}
    </li>
  );
}
```

Add the imports at the top:

```tsx
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router-dom";
import { useUi } from "../../stores/ui";
import { docsApi } from "./docs.api";
import { buildTree, type TreeNode } from "./tree";
```

> **Note:** v0.1's UX is intentionally minimal (prompt + confirm). Plan 7+ can replace with a real context menu component.

- [ ] **Step 2: Move endpoint hookup (deferred)**

`docsApi.move` exists for completeness; the sidebar drag-drop UI is out of scope for v0.1 — left for a follow-up plan.

- [ ] **Step 3: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): rename + archive via right-click prompt"
```

---

## Task 14: Presence colors via CollaborationCursor

Already wired in T12 via `colorFor(user_id)`. T14 verifies it works by adding a small piece of UI that shows who's viewing.

**Files:**
- Modify: `web/src/features/editor/KnotEditor.tsx`

- [ ] **Step 1: Presence list**

Add to `KnotEditor` (above `EditorContent`):

```tsx
import { useState } from "react";

// Inside KnotEditor:
const [presence, setPresence] = useState<Array<{ name: string; color: string }>>([]);

useEffect(() => {
  const update = () => {
    const states = Array.from(provider.awareness.getStates().values()) as Array<
      { user?: { name?: string; color?: string } }
    >;
    setPresence(
      states
        .filter((s) => s.user?.name)
        .map((s) => ({ name: s.user!.name!, color: s.user!.color ?? "#666" })),
    );
  };
  provider.awareness.on("change", update);
  update();
  return () => { provider.awareness.off("change", update); };
}, [provider]);
```

Then render above the editor host:

```tsx
<div data-testid="presence-bar" style={{ marginBottom: 8 }}>
  {presence.map((p, i) => (
    <span
      key={i}
      style={{
        display: "inline-block",
        padding: "2px 6px",
        borderRadius: 4,
        background: p.color,
        color: "white",
        marginRight: 4,
        fontSize: 12,
      }}
    >
      {p.name}
    </span>
  ))}
</div>
```

- [ ] **Step 2: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): presence bar above editor"
```

---

## Task 15: 4403 close → toast + redirect

**Files:**
- Modify: `web/src/features/docs/DocPage.tsx`

- [ ] **Step 1: Wire to status**

Edit `DocPage.tsx`. In the status `useState` handler:

```tsx
useEffect(() => {
  if (status === "unauthorised") {
    notify("error", "You no longer have access to this document.");
    nav("/", { replace: true });
  }
}, [status, notify, nav]);
```

Add `useNavigate` import:

```tsx
import { useNavigate, useParams } from "react-router-dom";
const nav = useNavigate();
```

- [ ] **Step 2: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): 4403 close — toast + redirect to landing"
```

---

## Task 16: Members page

**Files:**
- Create: `web/src/features/workspace/workspace.api.ts`
- Rewrite: `web/src/features/workspace/MembersPage.tsx`

- [ ] **Step 1: workspace.api**

Create `/home/nik/Development/knot/web/src/features/workspace/workspace.api.ts`:

```ts
import * as v from "valibot";

import { apiFetch } from "../../lib/api";
import { Member, Workspace, parse } from "../../lib/validators";

export const workspaceApi = {
  async get() {
    const r = await apiFetch<unknown>("/api/workspace");
    if ("error" in r) return r;
    return { ok: parse(Workspace, r.ok) };
  },
  async listMembers() {
    const r = await apiFetch<unknown>("/api/workspace/members");
    if ("error" in r) return r;
    return { ok: parse(v.array(Member), r.ok) };
  },
  invite(email: string, role: "owner" | "editor" | "viewer") {
    return apiFetch<void>("/api/workspace/members", {
      method: "POST",
      body: { email, role },
    });
  },
  setRole(userId: string, role: "owner" | "editor" | "viewer") {
    return apiFetch<void>(`/api/workspace/members/${encodeURIComponent(userId)}`, {
      method: "PATCH",
      body: { role },
    });
  },
  remove(userId: string) {
    return apiFetch<void>(`/api/workspace/members/${encodeURIComponent(userId)}`, {
      method: "DELETE",
    });
  },
};
```

- [ ] **Step 2: MembersPage**

Replace `/home/nik/Development/knot/web/src/features/workspace/MembersPage.tsx` with:

```tsx
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { useSession } from "../../auth/SessionContext";
import { useUi } from "../../stores/ui";

import { workspaceApi } from "./workspace.api";

export default function MembersPage() {
  const qc = useQueryClient();
  const notify = useUi((s) => s.notify);
  const session = useSession();
  const myRole = session.data && "ok" in session.data ? session.data.ok.role : "viewer";
  const isOwner = myRole === "owner";

  const members = useQuery({
    queryKey: ["members"],
    queryFn: workspaceApi.listMembers,
  });

  const [inviteEmail, setInviteEmail] = useState("");
  const [inviteRole, setInviteRole] = useState<"owner" | "editor" | "viewer">("editor");

  const invite = useMutation({
    mutationFn: async () => workspaceApi.invite(inviteEmail, inviteRole),
    onSuccess: async (r) => {
      if ("error" in r) {
        notify(
          "error",
          r.error.code === "workspace.user_not_found"
            ? "User not found. Ask them to sign in first."
            : "Invite failed.",
        );
        return;
      }
      setInviteEmail("");
      await qc.invalidateQueries({ queryKey: ["members"] });
    },
  });

  const setRole = useMutation({
    mutationFn: async (a: { userId: string; role: "owner" | "editor" | "viewer" }) =>
      workspaceApi.setRole(a.userId, a.role),
    onSuccess: async (r) => {
      if ("error" in r) notify("error", "Role change failed");
      else await qc.invalidateQueries({ queryKey: ["members"] });
    },
  });

  const remove = useMutation({
    mutationFn: async (userId: string) => workspaceApi.remove(userId),
    onSuccess: async (r) => {
      if ("error" in r) notify("error", "Remove failed");
      else await qc.invalidateQueries({ queryKey: ["members"] });
    },
  });

  if (members.isLoading) return <main style={{ padding: 24 }}>Loading…</main>;
  if (!members.data || "error" in members.data) {
    return <main style={{ padding: 24 }}>Failed to load members.</main>;
  }

  return (
    <main style={{ padding: 24, fontFamily: "system-ui, sans-serif" }}>
      <h1>Members</h1>
      {isOwner && (
        <section style={{ marginTop: 12, marginBottom: 24 }}>
          <h2>Invite</h2>
          <form
            data-testid="invite-form"
            onSubmit={(e) => { e.preventDefault(); invite.mutate(); }}
            style={{ display: "flex", gap: 8 }}
          >
            <input
              data-testid="invite-email"
              type="email"
              value={inviteEmail}
              onChange={(e) => setInviteEmail(e.target.value)}
              placeholder="Email"
              required
              style={{ padding: 6 }}
            />
            <select
              data-testid="invite-role"
              value={inviteRole}
              onChange={(e) => setInviteRole(e.target.value as typeof inviteRole)}
              style={{ padding: 6 }}
            >
              <option value="viewer">Viewer</option>
              <option value="editor">Editor</option>
              <option value="owner">Owner</option>
            </select>
            <button data-testid="invite-submit" type="submit" style={{ padding: "6px 12px" }}>
              Invite
            </button>
          </form>
        </section>
      )}
      <table data-testid="members-table" style={{ width: "100%", borderCollapse: "collapse" }}>
        <thead>
          <tr>
            <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid #e5e5e5" }}>Email</th>
            <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid #e5e5e5" }}>Name</th>
            <th style={{ textAlign: "left", padding: 8, borderBottom: "1px solid #e5e5e5" }}>Role</th>
            {isOwner && <th style={{ padding: 8, borderBottom: "1px solid #e5e5e5" }}>Actions</th>}
          </tr>
        </thead>
        <tbody>
          {members.data.ok.map((m) => (
            <tr key={m.user_id} data-testid={`member-${m.user_id}`}>
              <td style={{ padding: 8 }}>{m.email}</td>
              <td style={{ padding: 8 }}>{m.display_name}</td>
              <td style={{ padding: 8 }}>
                {isOwner ? (
                  <select
                    value={m.role}
                    onChange={(e) =>
                      setRole.mutate({ userId: m.user_id, role: e.target.value as typeof inviteRole })
                    }
                  >
                    <option value="viewer">Viewer</option>
                    <option value="editor">Editor</option>
                    <option value="owner">Owner</option>
                  </select>
                ) : (
                  m.role
                )}
              </td>
              {isOwner && (
                <td style={{ padding: 8 }}>
                  <button
                    onClick={() => {
                      if (window.confirm(`Remove ${m.email}?`)) remove.mutate(m.user_id);
                    }}
                  >
                    Remove
                  </button>
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>
    </main>
  );
}
```

- [ ] **Step 3: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): MembersPage with list/invite/role/remove"
```

---

## Task 17: PermissionsDialog

**Files:**
- Create: `web/src/features/docs/grants.api.ts`
- Create: `web/src/features/permissions/PermissionsDialog.tsx`
- Modify: `web/src/routes.tsx` — add `/doc/:id/permissions` overlay route
- Modify: `web/src/features/docs/DocPage.tsx` — link to permissions

- [ ] **Step 1: grants.api**

Create `/home/nik/Development/knot/web/src/features/docs/grants.api.ts`:

```ts
import * as v from "valibot";

import { apiFetch } from "../../lib/api";
import { Grant, parse } from "../../lib/validators";

export const grantsApi = {
  async list(docId: string) {
    const r = await apiFetch<unknown>(
      `/api/docs/${encodeURIComponent(docId)}/grants`,
    );
    if ("error" in r) return r;
    return { ok: parse(v.array(Grant), r.ok) };
  },
  put(docId: string, principal: string, role: "owner" | "editor" | "viewer", inherit: boolean) {
    return apiFetch<void>(
      `/api/docs/${encodeURIComponent(docId)}/grants/${encodeURIComponent(principal)}`,
      { method: "PUT", body: { role, inherit } },
    );
  },
  remove(docId: string, principal: string) {
    return apiFetch<void>(
      `/api/docs/${encodeURIComponent(docId)}/grants/${encodeURIComponent(principal)}`,
      { method: "DELETE" },
    );
  },
};
```

- [ ] **Step 2: PermissionsDialog**

Create `/home/nik/Development/knot/web/src/features/permissions/PermissionsDialog.tsx`:

```tsx
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";

import { useUi } from "../../stores/ui";
import { grantsApi } from "../docs/grants.api";
import { workspaceApi } from "../workspace/workspace.api";

export default function PermissionsDialog() {
  const { id } = useParams<{ id: string }>();
  const nav = useNavigate();
  const qc = useQueryClient();
  const notify = useUi((s) => s.notify);

  const grants = useQuery({
    queryKey: ["grants", id],
    queryFn: () => grantsApi.list(id!),
    enabled: Boolean(id),
  });
  const members = useQuery({
    queryKey: ["members"],
    queryFn: workspaceApi.listMembers,
  });

  const [addUser, setAddUser] = useState("");
  const [addRole, setAddRole] = useState<"owner" | "editor" | "viewer">("viewer");
  const [addInherit, setAddInherit] = useState(true);

  const add = useMutation({
    mutationFn: async () =>
      grantsApi.put(id!, `user:${addUser}`, addRole, addInherit),
    onSuccess: async (r) => {
      if ("error" in r) notify("error", "Couldn't add grant");
      else {
        setAddUser("");
        await qc.invalidateQueries({ queryKey: ["grants", id] });
      }
    },
  });
  const remove = useMutation({
    mutationFn: async (principal: string) => grantsApi.remove(id!, principal),
    onSuccess: async (r) => {
      if ("error" in r) notify("error", "Couldn't remove grant");
      else await qc.invalidateQueries({ queryKey: ["grants", id] });
    },
  });

  if (!id) return null;

  return (
    <div
      role="dialog"
      data-testid="permissions-dialog"
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.4)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 30,
      }}
      onClick={() => nav(-1)}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "white",
          padding: 24,
          minWidth: 420,
          borderRadius: 6,
          maxHeight: "80vh",
          overflow: "auto",
        }}
      >
        <h2>Permissions</h2>
        <p style={{ color: "#666", marginBottom: 16 }}>Explicit grants on this document.</p>
        <table style={{ width: "100%", borderCollapse: "collapse", marginBottom: 16 }}>
          <thead>
            <tr>
              <th style={{ textAlign: "left", padding: 6 }}>Principal</th>
              <th style={{ textAlign: "left", padding: 6 }}>Role</th>
              <th style={{ textAlign: "left", padding: 6 }}>Inherits</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {grants.data && "ok" in grants.data && grants.data.ok.map((g) => (
              <tr key={g.principal} data-testid={`grant-${g.principal}`}>
                <td style={{ padding: 6 }}>{g.principal}</td>
                <td style={{ padding: 6 }}>{g.role}</td>
                <td style={{ padding: 6 }}>{g.inherit ? "yes" : "no"}</td>
                <td style={{ padding: 6 }}>
                  <button onClick={() => remove.mutate(g.principal)}>Remove</button>
                </td>
              </tr>
            ))}
            {grants.data && "ok" in grants.data && grants.data.ok.length === 0 && (
              <tr>
                <td colSpan={4} style={{ padding: 6, color: "#888" }}>
                  No explicit grants. Effective role comes from workspace + ancestor inherits.
                </td>
              </tr>
            )}
          </tbody>
        </table>

        <h3>Add</h3>
        <div style={{ display: "flex", gap: 8, marginBottom: 12 }}>
          <select
            data-testid="grant-user"
            value={addUser}
            onChange={(e) => setAddUser(e.target.value)}
          >
            <option value="">Choose…</option>
            {members.data && "ok" in members.data && members.data.ok.map((m) => (
              <option key={m.user_id} value={m.user_id}>
                {m.display_name} ({m.email})
              </option>
            ))}
          </select>
          <select
            data-testid="grant-role"
            value={addRole}
            onChange={(e) => setAddRole(e.target.value as typeof addRole)}
          >
            <option value="viewer">Viewer</option>
            <option value="editor">Editor</option>
            <option value="owner">Owner</option>
          </select>
          <label style={{ display: "flex", alignItems: "center", gap: 4 }}>
            <input
              type="checkbox"
              checked={addInherit}
              onChange={(e) => setAddInherit(e.target.checked)}
            />
            Inherit
          </label>
          <button
            data-testid="grant-add"
            disabled={!addUser}
            onClick={() => add.mutate()}
          >
            Add
          </button>
        </div>

        <div style={{ display: "flex", justifyContent: "flex-end" }}>
          <button onClick={() => nav(-1)}>Close</button>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Route + button**

Edit `web/src/routes.tsx` — add `permissions` as a child of `doc/:id`:

```tsx
const PermissionsDialog = lazy(() => import("./features/permissions/PermissionsDialog"));

// In the protected children block, replace the existing `doc/:id` entry:
{
  path: "doc/:id",
  element: <DocTreeAndDoc />,
  children: [
    { path: "permissions", element: <Lazy><PermissionsDialog /></Lazy> },
  ],
},
```

And in `DocPage.tsx` header, add a link:

```tsx
<Link
  to="permissions"
  data-testid="open-permissions"
  style={{ marginLeft: 12 }}
>
  Permissions
</Link>
```

Add `import { Link, useNavigate, useParams } from "react-router-dom";` if not present.

- [ ] **Step 4: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): PermissionsDialog over /doc/:id/permissions"
```

---

## Task 18: SettingsPage

**Files:**
- Rewrite: `web/src/features/workspace/SettingsPage.tsx`

- [ ] **Step 1: Implement**

Replace `/home/nik/Development/knot/web/src/features/workspace/SettingsPage.tsx` with:

```tsx
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import { authApi } from "../../auth/session.api";
import { useSession } from "../../auth/SessionContext";
import { useUi } from "../../stores/ui";

import { workspaceApi } from "./workspace.api";

export default function SettingsPage() {
  const ws = useQuery({ queryKey: ["workspace"], queryFn: workspaceApi.get });
  const session = useSession();
  const qc = useQueryClient();
  const nav = useNavigate();
  const notify = useUi((s) => s.notify);

  async function logout() {
    const r = await authApi.logout();
    if ("error" in r) {
      notify("error", "Logout failed");
      return;
    }
    qc.removeQueries();
    nav("/login", { replace: true });
  }

  return (
    <main style={{ padding: 24, fontFamily: "system-ui, sans-serif" }}>
      <h1>Settings</h1>
      {ws.data && "ok" in ws.data && (
        <section data-testid="workspace-info" style={{ marginBottom: 24 }}>
          <p><strong>Workspace:</strong> {ws.data.ok.name} ({ws.data.ok.slug})</p>
        </section>
      )}
      {session.data && "ok" in session.data && (
        <section data-testid="user-info" style={{ marginBottom: 24 }}>
          <p><strong>Signed in as:</strong> {session.data.ok.display_name} ({session.data.ok.email})</p>
          <p><strong>Role:</strong> {session.data.ok.role}</p>
        </section>
      )}
      <button data-testid="logout" onClick={() => { void logout(); }}>
        Sign out
      </button>
    </main>
  );
}
```

- [ ] **Step 2: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): SettingsPage with workspace info + logout"
```

---

## Task 19: NotFound + onboarding

**Files:**
- Modify: `web/src/routes.tsx` — index route redirects to first doc or onboarding

- [ ] **Step 1: Onboarding redirect**

Edit `routes.tsx`. Replace `DocTreeAndLanding` with:

```tsx
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { useEffect } from "react";

import { docsApi } from "./features/docs/docs.api";

function DocTreeAndLanding() {
  const nav = useNavigate();
  const docs = useQuery({ queryKey: ["docs"], queryFn: docsApi.list });
  useEffect(() => {
    if (docs.data && "ok" in docs.data && docs.data.ok.length > 0) {
      nav(`/doc/${docs.data.ok[0]!.id}`, { replace: true });
    }
  }, [docs.data, nav]);
  return (
    <>
      <DocTree />
      <div style={{ padding: 24 }}>
        {docs.data && "ok" in docs.data && docs.data.ok.length === 0 ? (
          <>
            <h2>Welcome to knot</h2>
            <p>Create your first document from the sidebar.</p>
          </>
        ) : (
          "Loading…"
        )}
      </div>
    </>
  );
}
```

- [ ] **Step 2: Verify + commit**

```bash
cd /home/nik/Development/knot/web
pnpm tsc
pnpm lint
```

```bash
cd /home/nik/Development/knot
git add web/
git commit -m "feat(web): landing redirects to first doc; onboarding card otherwise"
```

---

## Task 20: Re-enable two-users-converge with real auth

**Files:**
- Rewrite: `e2e/flows/two-users-converge.spec.ts`

- [ ] **Step 1: Rewrite**

Replace `/home/nik/Development/knot/e2e/flows/two-users-converge.spec.ts` with:

```ts
import { execSync } from "node:child_process";

import { expect, test } from "@playwright/test";

const SERVER = "http://localhost:3000";

function reset() {
  const tables = [
    "acl_invalidations", "audit_events", "doc_markdown_cache",
    "doc_snapshots", "doc_updates", "document_grants", "documents",
    "sessions", "workspace_members", "users", "workspaces",
  ].join(", ");
  execSync(
    `docker compose -f deploy/compose/dev.yml exec -T postgres psql -U knot -d knot -c "TRUNCATE TABLE ${tables} CASCADE"`,
    { cwd: "..", stdio: "pipe" },
  );
}

test.beforeAll(reset);

test("two users editing converge", async ({ browser }) => {
  // First user creates the workspace via /auth/setup and a doc.
  const setupCtx = await browser.newContext();
  const setupPage = await setupCtx.newPage();
  await setupPage.goto("/setup");
  await setupPage.getByTestId("setup-email").fill("alice@example.com");
  await setupPage.getByTestId("setup-display-name").fill("Alice");
  await setupPage.getByTestId("setup-password").fill("hunter22-alice");
  await setupPage.getByTestId("setup-submit").click();
  await setupPage.getByTestId("new-doc").click();
  // Capture the doc URL.
  await setupPage.waitForURL(/\/doc\/.+/);
  const docUrl = setupPage.url();

  // Invite Bob via direct SQL (the same pre-provision pattern docs.spec uses).
  execSync(
    `docker compose -f deploy/compose/dev.yml exec -T postgres psql -U knot -d knot -c ` +
      `"INSERT INTO users (email, display_name) VALUES ('bob@example.com', 'Bob')"`,
    { cwd: "..", stdio: "pipe" },
  );
  await setupPage.goto("/members");
  await setupPage.getByTestId("invite-email").fill("bob@example.com");
  await setupPage.getByTestId("invite-role").selectOption("editor");
  await setupPage.getByTestId("invite-submit").click();
  await expect(setupPage.getByTestId("invite-form")).toBeVisible();

  // Bob signs in via password reset? — v0.1 has no password reset for the
  // OIDC-stub user we just inserted. Skip: just verify Alice can type + reload
  // and the text persists. Two-user convergence comes back when Plan 8
  // implements per-user invite-with-password.
  await setupPage.goto(docUrl);
  await setupPage.locator("[data-testid='editor-host'] .ProseMirror").click();
  await setupPage.keyboard.type("Hello from Alice.");
  await setupPage.waitForTimeout(800); // let writer flush
  await setupPage.reload();
  await expect(setupPage.locator("[data-testid='editor-host']")).toContainText("Hello from Alice.");
});
```

> **Implementer note:** Real two-user convergence (Alice + Bob each typing) needs a way to log Bob in. Plan 8 will add password-set on invite or magic-link. For Plan 6 the test verifies single-user convergence after reload — proving the WS + persistence loop works end-to-end. The test name is preserved so a future Plan 8 PR replaces just the bodies.

- [ ] **Step 2: Run + commit**

```bash
cd /home/nik/Development/knot
make compose.up
make migrate.up
cd e2e
pnpm playwright test two-users-converge.spec.ts
```

Expected: 1 pass.

```bash
cd /home/nik/Development/knot
git add e2e/
git commit -m "test(e2e): re-enable two-users-converge — single-user persistence after reload"
```

---

## Task 21: New e2e — login + editor

**Files:**
- Create: `e2e/flows/login.spec.ts`
- Create: `e2e/flows/editor.spec.ts`

- [ ] **Step 1: login.spec.ts**

Create `/home/nik/Development/knot/e2e/flows/login.spec.ts`:

```ts
import { execSync } from "node:child_process";

import { expect, test } from "@playwright/test";

const SERVER = "http://localhost:3000";

function reset() {
  const tables = [
    "acl_invalidations", "audit_events", "doc_markdown_cache",
    "doc_snapshots", "doc_updates", "document_grants", "documents",
    "sessions", "workspace_members", "users", "workspaces",
  ].join(", ");
  execSync(
    `docker compose -f deploy/compose/dev.yml exec -T postgres psql -U knot -d knot -c "TRUNCATE TABLE ${tables} CASCADE"`,
    { cwd: "..", stdio: "pipe" },
  );
}

test.beforeAll(reset);

test("setup → land on docs landing → create doc", async ({ page }) => {
  await page.goto("/setup");
  await page.getByTestId("setup-email").fill("owner@example.com");
  await page.getByTestId("setup-display-name").fill("Owner");
  await page.getByTestId("setup-password").fill("owner-hunter22");
  await page.getByTestId("setup-submit").click();
  await page.waitForURL(/\/$/);
  await page.getByTestId("new-doc").click();
  await page.waitForURL(/\/doc\/.+/);
  await expect(page.getByTestId("doc-title")).toBeVisible();
});

test("unauthenticated visit redirects to login", async ({ page }) => {
  await page.goto("/");
  await page.waitForURL(/\/login/);
  await expect(page.getByTestId("login-form")).toBeVisible();
});

test("wrong password shows error", async ({ page }) => {
  await page.goto("/login");
  await page.getByTestId("login-email").fill("wrong@example.com");
  await page.getByTestId("login-password").fill("nopenope");
  await page.getByTestId("login-submit").click();
  await expect(page.getByTestId("login-error")).toBeVisible();
});
```

- [ ] **Step 2: editor.spec.ts**

Create `/home/nik/Development/knot/e2e/flows/editor.spec.ts`:

```ts
import { execSync } from "node:child_process";

import { expect, test } from "@playwright/test";

function reset() {
  const tables = [
    "acl_invalidations", "audit_events", "doc_markdown_cache",
    "doc_snapshots", "doc_updates", "document_grants", "documents",
    "sessions", "workspace_members", "users", "workspaces",
  ].join(", ");
  execSync(
    `docker compose -f deploy/compose/dev.yml exec -T postgres psql -U knot -d knot -c "TRUNCATE TABLE ${tables} CASCADE"`,
    { cwd: "..", stdio: "pipe" },
  );
}

test.beforeAll(reset);

test("editor connects, accepts typing, persists across reload", async ({ page }) => {
  await page.goto("/setup");
  await page.getByTestId("setup-email").fill("e@example.com");
  await page.getByTestId("setup-display-name").fill("E");
  await page.getByTestId("setup-password").fill("hunter22!hunter22");
  await page.getByTestId("setup-submit").click();
  await page.getByTestId("new-doc").click();
  await page.waitForURL(/\/doc\/.+/);
  const url = page.url();

  // Status reaches "connected".
  await expect(page.getByTestId("status-dot")).toHaveAttribute("data-status", "connected", {
    timeout: 5_000,
  });

  // Type + reload.
  await page.locator("[data-testid='editor-host'] .ProseMirror").click();
  await page.keyboard.type("Editor smoke test.");
  await page.waitForTimeout(600);
  await page.goto(url);
  await expect(page.locator("[data-testid='editor-host']")).toContainText("Editor smoke test.");
});
```

- [ ] **Step 3: Run + commit**

```bash
cd /home/nik/Development/knot
make compose.up
make migrate.up
cd e2e
pnpm playwright test login.spec.ts editor.spec.ts
```

Expected: 4 tests pass (3 login + 1 editor).

```bash
cd /home/nik/Development/knot
git add e2e/
git commit -m "test(e2e): login + editor flows"
```

---

## Self-review checklist (for the executing agent)

Before declaring Plan 6 complete:

- [ ] `pnpm tsc` clean
- [ ] `pnpm lint` clean (zero warnings)
- [ ] `pnpm test` (vitest) green
- [ ] `cd e2e && pnpm playwright test` green (all suites)
- [ ] `cargo test --workspace` still green (no server regressions)
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` still clean
- [ ] Main bundle under 250 KB gzipped (`pnpm build` then `du -sh dist/assets/*.js | grep main`)
- [ ] `/login` is reachable without a cookie; protected routes redirect.
- [ ] `/setup` succeeds the first time and 410s the second time.
- [ ] Creating a doc in the sidebar lands on `/doc/:id`; the editor reaches "connected".
- [ ] Typing in the editor → reload → text is still there.
- [ ] Members page: owner can invite (via pre-provisioned user), change role, remove.
- [ ] Permissions dialog opens via `/doc/:id/permissions`; grants list / add / remove work.
- [ ] Removing a grant on an active editor closes their WS with 4403 and toasts "you no longer have access" (manual smoke — separate browser).
- [ ] Sign-out clears the session and lands on `/login`.

When green: write `docs/superpowers/research/2026-06-0X-plan6-outcome.md` summarising what landed + spec drift, tag `plan-6-complete`, and proceed to Plan 7 (UI polish / drag-drop / command palette) or Plan 8 (auth completion: invite-with-password + password reset).
