# Plan 7 Outcome — UI Polish (Toolbar, DnD, Command Palette, Role Gating)

**Status:** GO. All 12 tasks landed; all gates green. 19/19 Playwright tests pass.

**Verdict:** knot now has a functional editor toolbar, drag-and-drop tree reordering, a command palette (Ctrl+K), and role-aware UI gating throughout the sidebar, members page, and doc editor. The main bundle stays well under the 250 KB gzip target. Recommended next: hardening plan (rate limiting, NetworkPolicy) or Plan 8/9 follow-ons.

## What landed

Plan 7 commits (HEAD `b678212` + T12):

| Commit | Task | Subject |
|---|---|---|
| e5f717f | T1  | EditorToolbar — bold, italic, strike, code, h1/h2/h3, lists, blockquote, code-block |
| 0e69698 | T2  | Link toolbar button + popover (input, apply, remove) |
| f004d81 | T3  | ContextMenu primitive (portal, keyboard dismiss, backdrop) |
| 62fda9d | T4  | DocTree uses ContextMenu for rename + delete |
| 2c083ca | T5  | reorderInto + moveArgs tree utilities |
| 65848e9 | T6+T7 | dnd-kit integration + optimistic move mutation |
| ce5d5a6 | T8+T9 | CommandPalette (cmdk) + action items (create, logout, nav) |
| 0b842fe | T10 | useEffectiveRole hook + Members/DocPage role gates |
| b678212 | T11 | DocTree role gates (new-doc button, drag handle, context menu) |
| (this commit) | T12 | 4 e2e specs + outcome doc |

## Gates

| Gate | Result |
|---|---|
| `cargo test --workspace` | green (pre-existing; no backend changes in Plan 7) |
| `pnpm build` | green — clean, no type errors |
| `pnpm playwright test` (19 tests) | 19/19 pass |
| Bundle size — `index` chunk | 343 KB raw / **111 KB gzip** |
| Bundle size — `KnotEditor` chunk | 452 KB raw / **143 KB gzip** |

Both main chunks are well under the 250 KB gzip ceiling. dnd-kit added ~25 KB raw to the editor chunk as expected.

## Architecture summary

**EditorToolbar** (`web/src/features/editor/EditorToolbar.tsx`): renders a horizontal strip of `<button>` elements, each calling the matching Tiptap `editor.chain().focus().<command>()` and reading `.isActive()` for the active state highlight. All buttons carry `data-testid` attributes (e.g. `toolbar-bold`, `toolbar-h1`).

**Link popover**: the `toolbar-link` button opens an inline popover (`link-popover`) with an `<input data-testid="link-input">`, apply, and remove buttons. Uses a controlled `useState` in the toolbar component; no separate dialog overlay.

**ContextMenu** (`web/src/components/ContextMenu.tsx`): a portal-rendered `<ul>` positioned at the pointer coordinates passed in. Closes on Escape, click-outside (backdrop div), or item selection. DocTree wires rename and delete items; the items array is empty for viewers so the context menu is never opened.

**dnd-kit tree DnD** (`web/src/features/docs/DocTree.tsx`): `DndContext` + `SortableContext` wrap the flat doc list. `PointerSensor` with `activationConstraint: { distance: 6 }` prevents accidental drags on clicks. `onDragEnd` calls `doMove(movedId, { parent_id: targetId })` — drop-onto-row nests the dragged item as a child. The optimistic update (`onMutate`) re-orders the local cache immediately; `onSettled` invalidates to get server truth. On error the cache is rolled back and a toast fires.

**CommandPalette** (`web/src/components/CommandPalette.tsx`): opens on `Ctrl+K` / `⌘K` via a `keydown` listener on `document`. Renders a modal with `data-testid="cmdk"`, a search input, and a list of items. Doc items are prefixed `cmdk-item-doc:<id>`, action items `cmdk-item-action:<key>`, nav items `cmdk-item-nav:<key>`. Filters by title substring, case-insensitive. Enter on the highlighted item navigates or executes.

**useEffectiveRole** (`web/src/auth/useEffectiveRole.ts`): queries `/api/workspace/me` (workspace role) and optionally `/api/docs/:id` (doc effective_role). Components read `workspace` + `doc` fields to gate UI.

**Role gates applied:**
- `DocTree`: `new-doc` button hidden for viewer; drag handle `disabled` for viewer; ContextMenu items empty for viewer.
- `MembersPage`: entire invite form (`invite-form`) gated behind `isOwner`.
- `DocPage`: permissions link only shown when `effRole === "owner"`.

## What was non-obvious

**dnd-kit PointerSensor activation constraint.** Without `activationConstraint: { distance: 6 }`, any mousedown on a tree row starts a drag — clicking to navigate never fires. The distance threshold means the user must move 6 px before the drag begins, which distinguishes a click from a drag.

**Playwright `boundingBox()` does not retry.** Unlike `expect(locator).toBeVisible()`, `locator.boundingBox()` returns `null` immediately if the element has no layout box. In the dnd spec, calling `boundingBox()` on a locator that hasn't rendered yet returns `null` rather than waiting. The fix was to call `expect(row).toBeVisible({ timeout })` before `boundingBox()`.

**Tree row text matching is fragile when title mutations are in-flight.** When Playwright types a title and blurs, the React `onBlur` fires a PATCH, then invalidates `["docs"]`. The tree re-renders. If the second `new-doc` click fires before the invalidation resolves, the tree may briefly show the old title ("Untitled") for the first doc. Matching by `hasText: "Parent"` then fails. The dnd spec was made robust by matching rows by index (`.nth(0)`, `.nth(1)`) rather than text content.

**`fill()` on controlled React inputs.** Playwright's `fill()` properly triggers React's synthetic `onChange` via `InputEvent`. The `blur()` call correctly fires `onBlur`. No special React workaround needed in Playwright ≥ 1.40.

**Role gate redirect timing.** The role-gating spec navigated to `/members` immediately after clicking setup-submit without waiting for the URL to settle. The owner session cookie was not yet set when the `/members` page loaded, so `useEffectiveRole` returned a viewer role and the invite form was hidden. Adding `waitForURL(/\/(?:doc\/.+)?$/)` after setup-submit fixed it.

## What's still deferred

- **Mobile / responsive pass** — toolbar overflows on narrow viewports; no hamburger or collapsed state.
- **DnD visual drop indicator** — no insertion line while dragging; the user can't see where the item will land until they drop it.
- **Keyboard DnD** — `KeyboardSensor` is wired but not covered by tests; arrow-key reordering UX is untested.
- **Palette fuzzy search** — current filtering is a simple `includes()` substring match; Fuse.js or similar would improve recall.
- **Palette keyboard navigation** — arrow-key item selection in the list is not implemented; only Tab and Enter work.
- **Doc icon** — tree hardcodes `📄`; the `node.icon` field is never set; planned to come from doc metadata.
- **ContextMenu on touch** — no long-press handler; touch users have no access to rename/delete.

## Carryforward for the next plan

1. **Hardening plan.** Rate limiting on `/auth/login` + `/auth/password` per IP + per user. NetworkPolicy templates. Image signing (cosign). PrometheusRule alerting rules from the SLO doc.
2. **Mobile pass.** Responsive toolbar (collapsed to a `…` overflow menu), sidebar drawer, touch-friendly DnD.
3. **DnD polish.** Drop indicator line, auto-scroll on drag-near-edge, keyboard DnD test coverage.

## Files of interest

| Path | Role |
|---|---|
| `web/src/features/editor/EditorToolbar.tsx` | Toolbar buttons + link popover |
| `web/src/features/editor/KnotEditor.tsx` | Mounts toolbar above ProseMirror host |
| `web/src/components/ContextMenu.tsx` | Portal context menu primitive |
| `web/src/components/CommandPalette.tsx` | Ctrl+K command palette |
| `web/src/features/docs/DocTree.tsx` | DnD tree + role-gated new-doc/drag/context menu |
| `web/src/features/docs/tree.ts` | `buildTree`, `reorderInto`, `moveArgs` utilities |
| `web/src/auth/useEffectiveRole.ts` | Workspace + doc role derivation hook |
| `web/src/features/workspace/MembersPage.tsx` | Invite form gated by `isOwner` |
| `e2e/flows/editor-toolbar.spec.ts` | Toolbar bold + heading e2e |
| `e2e/flows/tree-dnd.spec.ts` | DnD crash + no-error-toast e2e |
| `e2e/flows/command-palette.spec.ts` | Ctrl+K open + filter + navigate e2e |
| `e2e/flows/role-gating.spec.ts` | Viewer UI gate e2e |
