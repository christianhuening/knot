# Excalidraw Boards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** First-class collaborative Excalidraw boards inside knot docs. A board lives in its own Yjs sub-document with its own room actor and WS route; the editor stores only a `{ boardId }` reference. Inline preview shows a server-cached SVG; clicking opens a modal full-canvas editor with concurrent presence cursors. Markdown round-trip uses a `knot://board/{id}.svg` sentinel.

**Architecture:**
- **Per-board Yjs doc.** New tables `boards`, `board_updates`, `board_snapshots` mirror the document tables. A board has a parent doc and inherits its ACL.
- **`BoardRoom` actor.** Mirrors `knot_crdt::Room` against a board's Y.Doc. Maintains awareness for presence cursors.
- **Two WS namespaces.** Existing `/collab/:doc_id` becomes `/collab/doc/:doc_id` (with a backwards-compatible alias for the bare path through the v0.1 cycle); new `/collab/board/:board_id` connects to a `BoardRoom`.
- **Y binding (Option A).** Y.Map<id, ElementJSON> keyed by Excalidraw element id; `onChange` reconciles the full snapshot to the map (last-write-wins per element). Plenty smooth for ≤3 concurrent editors; element-level granularity wins.
- **Inline preview.** Frontend debounces (300ms) on `onChange`, calls `excalidraw.exportToSvg`, PUTs the SVG to `PUT /api/boards/:id/svg`. Backend serves it back via `GET /api/boards/:id/svg`. NodeView shows the cached SVG (or a placeholder if not yet rendered).
- **Modal editor.** Click the inline preview → full-screen modal mounts the Excalidraw component bound to the board's Y.Doc + WS provider. Close persists the latest snapshot via the usual room flow.
- **Markdown sentinel.** Boards serialize as `![{label}](knot://board/{id}.svg)`; on import the parser reconstructs an `excalidraw_board` node from the sentinel URL.
- **Excalidraw lazy-imported** inside the NodeView (~2 MB chunk). Pages without boards never pay the cost.
- **Permissions.** Board ACL = parent doc ACL. No separate grants table for v1.
- **Public share.** Read-only Excalidraw renderer mounted from `PublicDoc` when the markdown contains the sentinel; backend serves the SVG via a public token-gated endpoint.

**Tech Stack:** `@excalidraw/excalidraw` ^0.18 (dynamic import), existing yrs/Yjs stack, axum, sqlx, Tiptap NodeView.

**Constraint — non-breaking:** Every existing `data-testid` and the 26-spec Playwright suite still passes. New testIds documented per task. Public-doc rendering path remains anonymous-readable.

---

## Backend tasks

### Task 1: Migration — boards / board_updates / board_snapshots

**Files:**
- Create: `migrations/<ts>_boards.sql`
- Modify: `crates/knot-storage/tests/migrations_apply.rs` (extend the hardcoded table list)

- [ ] **Step 1: SQL**

```sql
-- boards
CREATE TABLE boards (
  id           UUID PRIMARY KEY,
  doc_id       UUID NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  created_by   UUID NOT NULL REFERENCES users(id),
  label        TEXT NULL,
  svg_cached   BYTEA NULL,             -- last-known rendered preview
  svg_seq      BIGINT NOT NULL DEFAULT 0,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  deleted_at   TIMESTAMPTZ NULL
);
CREATE INDEX boards_doc_idx ON boards(doc_id) WHERE deleted_at IS NULL;

-- y-updates append log (mirrors doc_updates)
CREATE TABLE board_updates (
  board_id     UUID NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
  seq          BIGSERIAL NOT NULL,
  bytes        BYTEA NOT NULL,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (board_id, seq)
);

-- snapshots (mirrors doc_snapshots)
CREATE TABLE board_snapshots (
  board_id     UUID NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
  snapshot_seq BIGINT NOT NULL,
  state        BYTEA NOT NULL,
  byte_size    BIGINT NOT NULL,
  created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (board_id, snapshot_seq)
);
```

- [ ] **Step 2: Extend migrations_apply.rs**

Add `"boards"`, `"board_updates"`, `"board_snapshots"` to the expected list.

- [ ] **Step 3: cargo test -p knot-storage --test migrations_apply** — green.

- [ ] **Step 4: Commit** `feat: boards / board_updates / board_snapshots migration (Plan 25 T1)`.

---

### Task 2: knot-storage::boards — BoardStore trait + Pg impl

**Files:**
- Create: `crates/knot-storage/src/boards.rs`
- Modify: `crates/knot-storage/src/lib.rs` (re-export)

Trait surface — mirrors `DocumentStore` patterns:

```rust
#[async_trait]
pub trait BoardStore: Send + Sync + 'static {
    async fn create(&self, doc_id: Uuid, created_by: Uuid, label: Option<String>) -> Result<Board>;
    async fn get(&self, id: Uuid) -> Result<Board>;
    async fn list_for_doc(&self, doc_id: Uuid) -> Result<Vec<Board>>;
    async fn delete(&self, id: Uuid) -> Result<()>;

    /// Append a y-update; returns the new seq.
    async fn append_update(&self, id: Uuid, bytes: &[u8]) -> Result<i64>;
    /// Load all updates for replay on room boot.
    async fn load_updates(&self, id: Uuid) -> Result<Vec<Vec<u8>>>;

    /// Snapshot ops mirror doc_snapshots.
    async fn put_snapshot(&self, id: Uuid, seq: i64, state: &[u8]) -> Result<()>;
    async fn latest_snapshot(&self, id: Uuid) -> Result<Option<(i64, Vec<u8>)>>;

    /// SVG cache (set when the client PUTs a new preview).
    async fn set_svg(&self, id: Uuid, bytes: &[u8]) -> Result<()>;
    async fn get_svg(&self, id: Uuid) -> Result<Option<Vec<u8>>>;
}
```

- [ ] Implement PgBoardStore against the new tables.
- [ ] Integration tests under `crates/knot-storage/tests/boards.rs` using `knot_test_support::fresh_db`.
- [ ] `cargo test -p knot-storage` — green.
- [ ] Commit.

---

### Task 3: BoardRoom actor (generalize knot-crdt::Room or copy)

**Files:**
- Create: `crates/knot-crdt/src/board_room.rs`
- Modify: `crates/knot-crdt/src/lib.rs`

Mirrors `Room::run` but with three differences:

1. State persistence calls `BoardStore::append_update` / `BoardStore::put_snapshot`.
2. No markdown cache concerns (boards have no markdown — SVG is the export).
3. Awareness pipeline reused unchanged.

- [ ] Keep `Bus::publish` / `subscribe_presence` channels generic (suffix subjects with `board:{id}` vs `doc:{id}`).
- [ ] `RoomRegistry` becomes `<K: Key>` keyed by either doc or board id, or we ship a parallel `BoardRegistry`. **Decision:** parallel registry — simpler than introducing a trait gymnastic, fewer cross-cutting changes.
- [ ] Unit test: two BoardRooms for the same id share the same actor; sending an update from one client fans out to the other.
- [ ] Commit.

---

### Task 4: WS routes — `/collab/doc/:id` and `/collab/board/:id`

**Files:**
- Modify: `crates/knot-server/src/routes/collab.rs` (or wherever the WS upgrade lives)
- Modify: `crates/knot-server/src/main.rs` route mounts

- [ ] Rename existing `/collab/:doc_id` to `/collab/doc/:id`. Keep `/collab/:doc_id` as a **deprecated alias** that re-routes for one cycle — the frontend's `KnotProvider` will switch to the new URL in T9.
- [ ] Add `/collab/board/:id` that connects to a `BoardRoom` via the registry. Same handshake protocol as docs (y-protocol v1 sync + awareness + the new MSG_MENTION is doc-only and simply ignored on board sockets).
- [ ] ACL: viewer can subscribe (read updates) but apply_update is gated by editor+ on the parent doc. **Decision:** v1 keeps it editor-only WS connect; viewers see the cached SVG only.
- [ ] Integration test: connect, send a SYNC_STEP_1, get SYNC_STEP_2.
- [ ] Commit.

---

### Task 5: REST endpoints — create / list / delete / svg

**Files:**
- Create: `crates/knot-server/src/routes/api/boards.rs`
- Modify: `crates/knot-server/src/routes/api/mod.rs`

Surface:
- `POST   /api/docs/:doc_id/boards`          → 201 with `{ id, label }` (editor+)
- `GET    /api/docs/:doc_id/boards`          → list (viewer+)
- `DELETE /api/boards/:id`                   → 204 (editor+ on parent)
- `GET    /api/boards/:id/svg`               → image/svg+xml (viewer+)
- `PUT    /api/boards/:id/svg`               → 204 (editor+) — client uploads the rendered preview
- (public) `GET /p/{share_token}/boards/:id/svg` → image/svg+xml (no auth)

- [ ] ACL is computed from parent doc's effective role (reuse existing helpers).
- [ ] `Cache-Control: private, max-age=10` on the SVG GET to allow brief client-side caching without blocking edits.
- [ ] Integration tests.
- [ ] Commit.

---

### Task 6: Markdown sentinel — round-trip

**Files:**
- Modify: `crates/knot-markdown/src/to_markdown.rs`
- Modify: `crates/knot-markdown/src/from_markdown.rs`
- Modify: `tools/schema.json` + regenerate `crates/knot-markdown/src/schema.rs` / `web/src/features/editor/schema.ts`

Schema gets a new node kind `excalidraw_board` with attrs `{ board_id: string, label: string | null }`.

- [ ] Generator updates so both sides agree.
- [ ] `to_markdown`: on `excalidraw_board` emit:

```
![{label or "Diagram"}](knot://board/{board_id}.svg)
```

- [ ] `from_markdown`: detect images whose URL matches `knot://board/<uuid>.svg`, emit an `excalidraw_board` node instead of `image`.
- [ ] Round-trip fixture: `boards.md` containing a sentinel; serialise → parse → re-serialise produces the same text.
- [ ] `cargo test -p knot-markdown` — green.
- [ ] Commit.

---

## Frontend tasks

### Task 7: ExcalidrawBoard Tiptap node

**Files:**
- Create: `web/src/features/editor/nodes/ExcalidrawBoard.tsx`
- Modify: `web/src/features/editor/extensions.ts`
- Modify: `web/src/features/editor/schema.ts` (regenerated)

```tsx
import { Node, mergeAttributes } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";
import { ExcalidrawBoardView } from "./ExcalidrawBoardView";

export const ExcalidrawBoard = Node.create({
  name: "excalidraw_board",
  group: "block",
  atom: true,
  selectable: true,
  draggable: true,
  addAttributes() {
    return {
      board_id: { default: "" },
      label: { default: null },
    };
  },
  parseHTML() { return [{ tag: 'div[data-excalidraw-board]' }]; },
  renderHTML({ HTMLAttributes }) {
    return ["div", mergeAttributes(HTMLAttributes, { "data-excalidraw-board": "true" })];
  },
  addNodeView() { return ReactNodeViewRenderer(ExcalidrawBoardView); },
});
```

- [ ] Mount in `createExtensions`.
- [ ] Commit.

---

### Task 8: ExcalidrawBoardView — inline preview + modal trigger

**Files:**
- Create: `web/src/features/editor/nodes/ExcalidrawBoardView.tsx`
- Create: `web/src/lib/boards.api.ts`

```tsx
function ExcalidrawBoardView({ node }: ReactNodeViewProps) {
  const boardId = node.attrs.board_id as string;
  const [modalOpen, setModalOpen] = useState(false);
  const svg = useQuery({
    queryKey: ["board-svg", boardId],
    queryFn: () => boardsApi.getSvg(boardId),
    staleTime: 5_000,
  });
  return (
    <NodeViewWrapper className="my-3 rounded-md border border-border bg-surface overflow-hidden">
      <div className="px-3 py-1.5 border-b border-border bg-muted/40 flex items-center">
        <span className="text-[11px] font-semibold uppercase tracking-wider text-fg-muted">Diagram</span>
        <button className="ml-auto …" onClick={() => setModalOpen(true)}>Open</button>
      </div>
      <button className="block w-full p-3" onClick={() => setModalOpen(true)}>
        {svg.data && "ok" in svg.data
          ? <div dangerouslySetInnerHTML={{ __html: svg.data.ok }} />
          : <div className="h-40 grid place-items-center text-fg-muted text-sm">No preview yet — click to draw</div>}
      </button>
      {modalOpen && <ExcalidrawModal boardId={boardId} onClose={() => setModalOpen(false)} />}
    </NodeViewWrapper>
  );
}
```

- [ ] `boards.api.ts` mirrors `comments.api.ts` style: `create`, `list`, `getSvg`, `putSvg`.
- [ ] Commit.

---

### Task 9: BoardProvider — y-protocol WS provider for boards

**Files:**
- Create: `web/src/features/boards/BoardProvider.ts`

Forks KnotProvider against `/collab/board/:id`. Reuses the y-protocol encoding helpers. No MSG_MENTION channel.

- [ ] Awareness state holds `{ user, pointer }` for cursor sharing.
- [ ] Reconnect with exponential backoff (same as KnotProvider).
- [ ] Commit.

---

### Task 10: ExcalidrawModal + Y binding (Option A)

**Files:**
- Create: `web/src/features/boards/ExcalidrawModal.tsx`
- Create: `web/src/features/boards/yBinding.ts`

Modal:
- Mounts a `Y.Doc` + `BoardProvider`.
- Lazy-imports `@excalidraw/excalidraw`:

```ts
const Excalidraw = lazy(() =>
  import("@excalidraw/excalidraw").then((m) => ({ default: m.Excalidraw })),
);
```

- Y binding:

```ts
// yBinding.ts
export function bindExcalidraw(api: ExcalidrawImperativeAPI, ydoc: Y.Doc) {
  const elements = ydoc.getMap<unknown>("elements");
  let suppressOnChange = false;

  // Y → Excalidraw (initial + remote updates)
  function pushToExcalidraw() {
    if (suppressOnChange) return;
    const arr = Array.from(elements.values()) as ExcalidrawElement[];
    api.updateScene({ elements: arr });
  }
  elements.observeDeep(pushToExcalidraw);
  pushToExcalidraw();

  // Excalidraw → Y (last-write-wins per element id)
  function onChange(next: readonly ExcalidrawElement[]) {
    suppressOnChange = true;
    ydoc.transact(() => {
      const nextIds = new Set<string>();
      for (const el of next) {
        nextIds.add(el.id);
        const prev = elements.get(el.id) as ExcalidrawElement | undefined;
        if (!prev || prev.version !== el.version) {
          elements.set(el.id, structuredClone(el));
        }
      }
      // Remove elements that disappeared from the local snapshot.
      for (const id of elements.keys()) {
        if (!nextIds.has(id)) elements.delete(id);
      }
    });
    suppressOnChange = false;
  }
  return { onChange };
}
```

- Awareness binds pointer to Excalidraw's `onPointerUpdate` and renders remote pointers via `Excalidraw.collaborators`.

- Save-on-close: after the modal unmounts the SVG is exported and PUT to `/api/boards/:id/svg`.

- [ ] Commit.

---

### Task 11: SVG export debounce + cache invalidation

**Files:**
- Modify: `web/src/features/boards/ExcalidrawModal.tsx`

- [ ] Debounce 300ms on `onChange`, call `exportToSvg`, PUT bytes. Failure is non-fatal (toast: "Couldn't update preview").
- [ ] On PUT success, `qc.invalidateQueries(["board-svg", boardId])` so other open NodeViews refresh.
- [ ] Commit.

---

### Task 12: Toolbar — Insert diagram (Excalidraw) button

**Files:**
- Modify: `web/src/features/editor/EditorToolbar.tsx`

- [ ] New button between `toolbar-mermaid` and `Sep`. Use `PenSquare` or `Shapes` from Lucide. testid `toolbar-excalidraw`.
- [ ] On click: `POST /api/docs/:doc_id/boards`, insert `excalidraw_board` node with the new id, open the modal.
- [ ] Commit.

---

### Task 13: Public share — read-only board renderer

**Files:**
- Modify: `crates/knot-server/src/routes/public.rs`
- Modify: `web/src/features/public/PublicDoc.tsx`

- [ ] Public route resolves sentinel-image URLs `knot://board/{id}.svg` to absolute share URLs `/p/{token}/boards/{id}/svg` during markdown → HTML render.
- [ ] No board WS for public viewers — they get the SVG snapshot, period.
- [ ] Commit.

---

### Task 14: Playwright e2e — collaborative board

**Files:**
- Create: `e2e/flows/excalidraw.spec.ts`

- [ ] Setup → new doc → click `toolbar-excalidraw` → modal opens → draw a rectangle → close → assert inline preview renders.
- [ ] Two-context test: B opens the doc, opens the same board modal, asserts the rectangle is visible. Both draw a second shape; assert both shapes are visible in both contexts.
- [ ] Markdown export: GET `/api/docs/:id/markdown`, assert the body contains `knot://board/{uuid}.svg`.
- [ ] Commit.

---

### Task 15: Outcome doc + README row

**Files:**
- Create: `docs/superpowers/research/2026-06-03-plan25-outcome.md`
- Modify: `docs/superpowers/README.md` (add the row)

Capture gates, what landed, what was non-obvious (Y binding LWW edge cases, Excalidraw bundle-split layout, SVG export timing, awareness pointer format), and carryforward.

---

## Open trade-offs to revisit

- **Option B Yjs binding** (per-attribute) is a follow-up only if 2-finger conflict UX becomes a complaint.
- **Public WS access** (anonymous can connect read-only) is rejected for v1 — viewers see the snapshot SVG and a "Sign in to edit" overlay if they click.
- **Excalidraw's own collaboration UI** (the "Live collaboration" button) is **hidden** — we own the lifecycle via the modal.
- **Board export to PNG/PDF** deferred.
- **Sub-board copy/paste, board templates** deferred.

## Carryforward (post-Plan-25)

1. Plan 26 — Per-attribute Yjs binding for smoother concurrent shape edits.
2. Plan 27 — Board comments (drop a comment on a shape, anchored via element id).
3. Plan 28 — Board templates + insert from gallery.
