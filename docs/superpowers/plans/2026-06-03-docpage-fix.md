# DocPage Stale-State Fix Implementation Plan (Plan 14.5)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans (this is a 3-task micro-plan, inline execution is fine).

**Goal:** Fix the DocPage stale-title bug surfaced by Plan 14's e2e: when the user navigates from `/doc/A` to `/doc/B`, the title `<input>` still shows A's title until they blur or reload. Root cause: `DocPage` uses `useState("")` + an effect that hydrates from the doc query, but the effect's guard prevents re-sync on docId change. Two fixes are possible:
1. Reset state explicitly when `id` changes (extra effect).
2. Add `key={id}` higher in the tree so React remounts DocPage on docId change.

We're going with **option 2** — `key={id}` on the lazy editor + a small change to DocPage that derives `title` from the doc data instead of holding it in local state (since the input is debounced via blur, not keystroke). Cleaner than yet another effect.

**Predecessor:** Plan 14 (search, HEAD `be96f2d`).

---

## Tasks

| # | Title | LOC ≈ |
|---|---|---|
| 1 | Fix DocPage state management | 40 |
| 2 | Restore natural UI flow in search + command-palette e2e | 60 |
| 3 | Outcome doc | 0 |

---

## Task 1: Fix DocPage

**Files:**
- Modify: `web/src/features/docs/DocPage.tsx`

Approach: replace the `useState("")` + effect with `useState(() => meta.title)` resetting via `key`-prop on a sub-component. Actually simpler: just key the editor block on `id` so it remounts, and key the title input on `id` so its uncontrolled-default rerenders.

The cleanest minimal change: split the title input into a separate `DocTitle({ id, initialTitle })` component, mount it with `key={id}`. The component holds local edit state internally. When `id` changes, React unmounts and remounts, getting fresh `initialTitle`.

```tsx
function DocTitle({ id, initialTitle }: { id: string; initialTitle: string }) {
  const qc = useQueryClient();
  const notify = useUi((s) => s.notify);
  const [title, setTitle] = useState(initialTitle);
  const rename = useMutation({
    mutationFn: async (next: string) => docsApi.patch(id, { title: next }),
    onSuccess: async (r) => {
      if ("error" in r) {
        notify("error", "Couldn't rename");
        return;
      }
      await qc.invalidateQueries({ queryKey: ["docs"] });
      await qc.invalidateQueries({ queryKey: ["doc", id] });
    },
  });
  return (
    <input
      data-testid="doc-title"
      value={title}
      onChange={(e) => setTitle(e.target.value)}
      onBlur={() => { if (title !== initialTitle) rename.mutate(title); }}
      style={{
        border: "none",
        fontSize: 24,
        fontWeight: 600,
        flex: 1,
        background: "transparent",
      }}
    />
  );
}
```

In `DocPage`:
- Remove the `title` state + the sync effect.
- Remove the `rename` mutation (it moves into `DocTitle`).
- Render `<DocTitle id={id} initialTitle={meta.title} key={id} />`.

This works because `key={id}` forces React to unmount/remount the `DocTitle` instance whenever the route param changes. Each fresh mount initializes its `useState` with the new `initialTitle` prop.

Verify:

```bash
cd /home/nik/Development/knot/web
pnpm tsc && pnpm lint && pnpm test
```

Commit:

```bash
git add web/
git commit -m "fix(web): DocPage title resets on docId change (key-based remount)"
```

---

## Task 2: Restore natural UI flow in e2e

**Files:**
- Modify: `e2e/flows/search.spec.ts`
- Modify: `e2e/flows/command-palette.spec.ts`

Both specs used `page.evaluate(fetch /api/docs)` to work around the bug. Now they can use the natural UI path again:

```ts
for (const t of [...]) {
  await page.getByTestId("new-doc").click();
  await page.waitForURL(/\/doc\/.+/);
  const input = page.locator("[data-testid='doc-title']");
  await expect(input).toHaveValue("Untitled");
  const patch = page.waitForResponse(
    (r) => r.url().includes("/api/docs/") && r.request().method() === "PATCH",
  );
  await input.fill(t);
  await input.blur();
  await patch;
}
```

Run both specs + the full suite to confirm nothing regressed:

```bash
cd /home/nik/Development/knot/e2e
pnpm playwright test search.spec.ts command-palette.spec.ts
pnpm playwright test
```

Commit:

```bash
git add e2e/
git commit -m "test(e2e): restore natural UI flow now that DocPage title resets"
```

---

## Task 3: Outcome doc

Brief outcome doc; add a Plan 14.5 row to `docs/superpowers/README.md`.

```bash
git add docs/
git commit -m "docs: Plan 14.5 outcome — DocPage stale-state fix"
```
