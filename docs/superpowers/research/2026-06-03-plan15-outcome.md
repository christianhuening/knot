# Plan 15 Outcome — Mobile / Responsive

**Status:** GO. 6 tasks, 4 logic commits.

**Verdict:** knot is now usable on a phone. Sidebar drawer + full-screen palette are the highest-impact changes; toolbar wrap and touch-friendly tree are explicitly deferred. **23/23 e2e (was 22) with a new 375×667 mobile spec.**

## What landed

| Commit | Subject |
|---|---|
| b6e1f12 | feat(web): useViewport hook (mobile|tablet|desktop) + @types/node |
| d13e235 | feat(web): AppShell drawer + hamburger toggle on mobile viewports |
| ee33bb9 | feat(web): palette full-screen on mobile viewports |
| 9e70106 | test(e2e): mobile viewport — drawer + palette full-screen |

## Gates

- `pnpm tsc`, `pnpm lint`, `pnpm test` (vitest +1) — clean
- `pnpm playwright test` — **23 passed, 0 skipped**
- Existing desktop specs unchanged (all 22 still green)
- Bundle still well under 250 KB gz (no new runtime deps; only `@types/node` as dev)

## Architecture summary

**Breakpoints:** `mobile < 640`, `tablet 640–1024`, `desktop ≥ 1024`. Tablet falls through to desktop styling — the only special-cased bucket today is mobile.

**`useViewport()` hook** — single source of truth, `useState` initialized synchronously from `window.innerWidth`, updates on `resize`. Returns one of three literal strings. SSR-safe (defaults to `desktop` when `window` is undefined, even though knot is SPA-only and never SSRs).

**AppShell** branches inline styles on `vp === "mobile"`:
- Layout: `display: block` (instead of `grid`) so `<main>` takes the full viewport.
- Sidebar: `position: fixed; left: 0|−280px; z-index: 30; transition: left 200ms`. Slide-in animation.
- Backdrop: dark overlay at `z-index: 20`, click closes drawer.
- Hamburger toggle: top-left fixed button at `z-index: 25`, only rendered when `!sidebarOpen` so it doesn't overlap the open drawer.

**CommandPalette** branches the outer dialog and inner card on `vp === "mobile"`:
- Outer: white background instead of translucent black, `align-items: stretch`, no top padding.
- Inner card: `100vw × 100vh`, `border-radius: 0`, no shadow. The input + list still scroll inside.

The mobile soft keyboard now has room — previously the centered 480 px card stayed where it was and the keyboard would push it out of view.

## What was non-obvious

**Backdrop z-index gotcha.** First version had the sidebar at `z-index: 30` over the backdrop at `z-index: 20`. Logically correct (sidebar is on top) but it broke the e2e: Playwright tried to click the backdrop to close the drawer, hit the overlapping sidebar instead, and timed out. Fixed in the test by clicking with explicit coordinates outside the sidebar's 260 px width. Real users tap further out from the drawer than that, so this is a test-pragmatic fix, not a UX bug.

**`@types/node` was missing.** The Plan 12.5 `vite.config.ts` change introduced `process.env.VITE_COLLAB_VIA_PROXY`. Vite handles `process.env` natively in its config at runtime but `pnpm tsc` complained. Adding `@types/node` as a dev-dep is the standard fix. Caught when this plan's `pnpm tsc` failed for an unrelated reason.

**`useViewport` initial value matters.** Started with `useState<Viewport>("desktop")` and then synced via effect. That caused a "wrong viewport on first paint" flash on real mobile devices. Fixed by initializing synchronously from `window.innerWidth` in the `useState(() => classify(...))` factory.

**Zustand `sidebarOpen` default is `true`.** The store ships with `sidebarOpen: true` for desktop. On mobile that means the drawer is open on first page load — slightly weird but acceptable for v0.1; user dismisses it once and the state survives until reload. Adding "auto-close on mobile mount" would need a useEffect in AppShell; left as a small polish item.

## What's still deferred

- **Toolbar wrap + compact icons.** Editor toolbar overflows on narrow viewports; functional but ugly. Out of scope per user.
- **Touch-friendly tree.** Drag and the right-click context menu both rely on desktop ergonomics. Mobile users would benefit from explicit drag handles and a long-press menu. Out of scope per user.
- **Auto-close drawer on mobile mount.** Minor polish; mention above.
- **Tablet bucket specialization.** Currently falls through to desktop. Probably fine — tablets in landscape mode have plenty of room.
- **Orientation change handling.** Window `resize` covers this on most browsers; manual test if it ever flakes.
- **iOS Safari viewport quirks.** Bottom-bar disappearing on scroll, etc. Not validated on real iOS.

## Carryforward

Mobile foundation is in. Next plans (17 share links, 20 doc history, 19 comments) can lean on `useViewport()` for any mobile-specific styling they introduce. Continue with **Plan 17 (public share links)** — the biggest product unlock from the remaining list.

## Files

| Path | Role |
|---|---|
| `web/src/hooks/useViewport.ts` | the hook |
| `web/src/hooks/useViewport.test.ts` | vitest smoke |
| `web/src/components/AppShell.tsx` | drawer layout + hamburger |
| `web/src/components/CommandPalette.tsx` | full-screen on mobile |
| `e2e/flows/mobile.spec.ts` | new — 375×667 drawer + palette |
| `web/package.json` | added `@types/node` dev-dep |
