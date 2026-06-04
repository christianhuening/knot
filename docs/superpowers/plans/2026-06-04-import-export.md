# Import / Export Plan (Plan 32)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** A portable zip archive that captures a single doc, a subtree, or a whole workspace — markdown + attachments + boards — and can be imported back into a fresh knot instance. Useful for backup, migration, and offline editing round-trips.

**Architecture:**
- **Zip layout:**
  ```
  index.json                          # tree + manifest
  docs/<doc_id>.md                    # markdown body (round-trip-quality)
  attachments/<blob_id>.<ext>         # blob bytes
  boards/<board_id>.svg               # cached preview
  boards/<board_id>.yjs               # Yjs state bytes (encode_state_as_update_v1)
  ```
- **`index.json` schema:**
  ```json
  {
    "knot_export_version": "1",
    "workspace": { "id": "...", "name": "..." },
    "exported_at": "...",
    "docs": [
      { "id": "...", "parent_id": null, "title": "...", "position": 0 },
      ...
    ],
    "attachments": [{ "id": "...", "doc_id": "...", "filename": "...", "content_type": "..." }],
    "boards": [{ "id": "...", "doc_id": "...", "label": null }]
  }
  ```
- **Markdown contains the existing knot:// sentinels** for boards and (post-Plan 28) regular image URLs pointing at `/api/blobs/<id>`. On import, the importer rewrites all such IDs from old → new (a UUID remap table built during import).
- **Yjs seeding on import:** for each doc, render markdown→ProseMirror via existing `knot-markdown::from_markdown`, then construct a Yjs Doc whose text/structure matches, then `encode_state_as_update_v1` + insert into `doc_updates` so the first WS connect resumes from that state. Same for boards if their `.yjs` is present; otherwise boards start empty with their SVG as preview only.

---

## Tasks

### T1: Index manifest types + zip layout
- `crates/knot-server/src/export/mod.rs` — types for `IndexManifest`, `DocEntry`, `AttachmentEntry`, `BoardEntry`.
- Versioning: `knot_export_version = "1"`, future versions must remain backward-compatible.

### T2: Export endpoints
- `GET /api/docs/:id/export?include_descendants=true|false` — zips a single doc or a subtree.
- `GET /api/workspaces/me/export` — zips the whole workspace (owner-only).
- Stream the zip body (`zip` or `async-zip` crate). Use multipart-style read of attachments from blob storage.
- ACL: any included doc must be readable by the requester.

### T3: Importer scaffolding
- `POST /api/workspaces/me/import` — accepts multipart upload of a zip.
- Parse `index.json`, validate `knot_export_version`.
- Build a UUID remap table: old_doc_id → new_doc_id, old_blob_id → new_blob_id, old_board_id → new_board_id.
- Stream each docs/<id>.md through `knot_markdown::from_markdown` to produce a PM document tree.

### T4: Sentinel rewriting on import
- After parsing markdown, walk the PM tree and rewrite each `knot://board/<old>` → `knot://board/<new>` and each `/api/blobs/<old>` → `/api/blobs/<new>`.

### T5: Yjs seeding
- For each doc: build a Yjs doc from the PM tree (use the same converter that powers `from_markdown` → text). Encode as v1 update. Insert into `doc_updates` (seq=1).
- For each board: if `<id>.yjs` is present, decode + persist as the board's first update. Otherwise initialize empty.

### T6: Attachment + board SVG upload during import
- Stream each `attachments/<id>` into the configured blob store, recording new IDs.
- Stream each `boards/<id>.svg` into the boards SVG cache.

### T7: e2e roundtrip
- `e2e/flows/import-export.spec.ts`: create a doc with a board + image + child doc + checklist, export, import into a fresh workspace (different user / DB reset), assert all content present.

### T8: CLI helper (optional)
- A `knot-cli export --workspace --out file.zip` and `knot-cli import --in file.zip` for ops use cases. Defer if scope creeps.

### T9: Outcome doc

---

## Open trade-offs
- **Cross-instance permissions** — the export carries no ACL; on import everything is owned by the importing user. Stakeholder access has to be re-granted manually.
- **Incremental import / merge** (re-importing into an existing workspace and updating in place) — deferred; v1 always creates new IDs.
- **End-to-end encryption** of the zip — deferred; zip is plaintext, treat it like any backup.
- **Limits** — large workspaces could produce huge zips. Add a `max_workspace_export_bytes` guard + 504 if exceeded.
