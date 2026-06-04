# Attachments Polish Plan (Plan 27)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Surface explicit "Insert attachment" affordances in the editor. The `AttachmentNode` and blob upload pipeline from Plan 13 already work via drag-and-drop / paste; this plan only adds discoverability.

**Architecture:** No new backend, no new node, no new storage. Two affordances + minor UX polish.

---

## Tasks

### T1: Toolbar button — Attach file
- Modify `web/src/features/editor/EditorToolbar.tsx`.
- Add a `Paperclip` (Lucide) icon button between the existing image/diagram cluster and `Sep`. testId `toolbar-attachment`.
- On click: open a hidden `<input type="file" multiple>` programmatically; on selection, call `blobsApi.upload(docId, file)` for each file and insert one `Attachment` node per result at the current selection.
- For images among the chosen files, route through the existing `setImage` path (same as paste/drag) so they become image nodes, not attachment chips.

### T2: Slash command `/attach`
- The codebase doesn't have a generic slash menu yet. Add a minimal `SlashCommandExtension` that listens for `/` at the start of an empty paragraph and shows a popup with: `/attach`, `/diagram`, `/heading 1/2/3`, `/quote`, `/code`.
- Each item resolves to the same handler the toolbar button calls.
- (Optional: defer the slash menu to its own micro-plan if scope creeps. T2 can ship as toolbar-only if the slash menu isn't trivial.)

### T3: Empty-state hint inside the editor
- When the doc is empty, the first paragraph shows a placeholder line "Type / for commands, or just start writing." Tiptap's `Placeholder` extension handles this — wire it.

### T4: e2e — extend `upload-image.spec.ts`
- Add a non-image upload via the new toolbar button; assert an attachment chip is rendered.

### T5: Outcome doc

---

## Open trade-offs
- **Browse existing attachments** (re-insert a previously uploaded file without re-uploading) — deferred. Requires a `/api/docs/:id/attachments` listing endpoint and a picker UI.
- **Attachment previews** for PDFs / videos — deferred.
