# Images in Markdown Plan (Plan 28)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Round-trip ordinary images through the markdown serializer. Today paste/drag inserts a Tiptap `image` node but the markdown serializer drops it. After this plan, every `image` survives export and import, including in public shares.

**Architecture:**
- New schema node `image` (block, atom, attrs `{ src, alt, title? }`).
- Knot stores image blobs as attachments (already does — `blobsApi.upload`), so `src` is typically `/api/blobs/<uuid>` (signed-cookie-gated). External images (`https://…`) are also valid.
- Markdown: `![alt](src "title")`.
- Public share: rewrite `/api/blobs/<uuid>` URLs to a new public, token-gated endpoint `/p/<token>/blobs/<uuid>` so anonymous viewers see images. External images pass through.
- Tiptap's existing `@tiptap/extension-image` stays — only its `name` and attribute defaults need to align with the schema.

---

## Tasks

### T1: Schema — `image` node
- `tools/schema.json`: add `image` (block, atom, attrs `src` (required), `alt` (default null), `title` (default null)).
- Regen via `make schema.gen`.

### T2: knot-markdown — emit/parse
- `to_markdown.rs`: on `image`, emit `![<alt or "">](src<sp>"title")`.
- `from_markdown.rs`: handle `Tag::Image` non-sentinel case (sentinel-image branch for boards must run FIRST, then fall through to a generic image emit). Add `image_depth` already tracked. Build an `image` node with attrs.
- Round-trip fixtures (`images.md`).

### T3: Tiptap extension config
- `web/src/features/editor/extensions.ts`: keep `Image` import but configure with the schema-aligned name (`image`). Most defaults already match.
- Update `KnotEditor.tsx` paste/drag handler to use the schema name.

### T4: Public blob endpoint
- Backend: `GET /p/:token/blobs/:blob_id`. Validate the share token is alive; verify the blob belongs to the shared doc (via attachments mapping). Serve with original content-type + `Cache-Control: public, max-age=60`.
- In `routes/public.rs::render_markdown`, rewrite image `src` URLs starting with `/api/blobs/` to `/p/<token>/blobs/<id>`.
- External image URLs pass through unchanged.

### T5: e2e — `images.spec.ts`
- Paste an image, refresh, image still rendered.
- Export markdown via `GET /api/docs/:id/markdown`, assert the body contains `![<alt>](/api/blobs/<uuid>)`.
- Public-share path: open the share URL, assert the image src has been rewritten to `/p/<token>/blobs/...`.

### T6: Outcome doc

---

## Open trade-offs
- **Inline image dimensions** (`{width}` extension) — deferred.
- **External image caching/proxy** for privacy — deferred.
- **Alt-text editing UI** — deferred; alt is captured at paste time from clipboard metadata when present.
