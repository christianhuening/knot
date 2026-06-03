# Plan 14 Outcome — Full-Text Search

**Status:** GO. All 8 tasks landed; all gates green.

**Verdict:** ⌘K now searches every doc in your workspace by title and body, with ACL filtering and Postgres-rendered snippets. No new search service, no embeddings, no Lucene — just `tsvector` columns + GIN indexes + a single SQL query. Recommended next: **Plan 12.5 (chaos coverage)** for WS reconnect under flap, or **Plan 15 (mobile pass)**.

## What landed

Plan 14 commits (HEAD `8c66a12`):

| Commit | Task | Subject |
|---|---|---|
| 7c5daf4 | T1 | migrations: FTS — tsvector + GIN on title and cached body |
| 6cc8d5d | T2 | `knot-storage`: SearchStore trait + PgSearchStore (Postgres FTS) |
| 326ab4c | T3 | `knot-server`: GET /api/search — title + body with ACL filtering |
| c2a334b | T4 | server integration tests — title/body/snippet/short-query/anon/limit |
| 8d4b942 | T5 | `web`: searchApi.query |
| e704e3f | T6 | `web`: command palette server-side search with debounce + snippets |
| 8c66a12 | T7 | e2e: search + adapt palette spec to FTS semantics |

T8 is this outcome doc.

## Gates

- `cargo test --workspace` — green (+6 search integration cases)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean
- `pnpm tsc/lint/test` — clean
- `pnpm playwright test` — **21 passed, 1 skipped** (the Plan 12 WS reconnect spec, unchanged)
- `pnpm build` — main bundle **111 KB gzipped** (unchanged — no new deps in T5/T6)
- `make migrate.up` applied the new migration cleanly

## Architecture summary

**Index source:** existing `doc_markdown_cache.markdown_text` for body (Plan 5 cache, lazy-filled on export, lags live edits by one snapshot) + `documents.title` for title. Both get `STORED GENERATED` `tsvector` columns (english) + GIN indexes.

**Query** (in `crates/knot-storage/src/search.rs`):

```sql
SELECT d.id, d.parent_id, d.title,
       CASE WHEN c.body_tsv @@ plainto_tsquery('english', $2)
            THEN ts_headline('english', c.markdown_text,
                             plainto_tsquery('english', $2),
                             'MaxFragments=2,MinWords=5,MaxWords=15,StartSel=<b>,StopSel=</b>')
            ELSE NULL END AS snippet,
       GREATEST(
         COALESCE(ts_rank_cd(d.title_tsv, plainto_tsquery('english', $2)), 0.0) * 2.0,
         COALESCE(ts_rank_cd(c.body_tsv,  plainto_tsquery('english', $2)), 0.0)
       )::real AS rank
  FROM documents d
  LEFT JOIN doc_markdown_cache c ON c.doc_id = d.id
 WHERE d.workspace_id = $1
   AND d.archived_at IS NULL
   AND (d.title_tsv @@ plainto_tsquery('english', $2)
        OR c.body_tsv @@ plainto_tsquery('english', $2))
 ORDER BY rank DESC LIMIT $3
```

Title rank is boosted ×2 because title matches are usually what users mean. `ts_rank_cd` (cover-density) handles mixed title+body matches reasonably.

**Server endpoint:** `GET /api/search?q=<query>&limit=<n>`. Short queries (<2 chars) short-circuit to empty `{ results: [] }` without hitting the DB. Hard limit clamped to 20. ACL re-check `effective_role` for every candidate so revoked grants don't leak through. The handler over-fetches (`limit * 2`) to leave room for the ACL filter and stops once the post-filter count hits `limit`.

**Frontend:**
- `searchApi.query(q, limit)` is a thin wrapper around `apiFetch` returning `ApiResult<SearchHit[]>`.
- `CommandPalette.tsx` keeps its static nav/action items (Create, Members, Settings, Sign out) client-side. For `q.length >= 2` it debounces 200 ms, fires `searchApi.query`, and renders results above the nav items with snippets shown below the title.
- Snippets contain `<b>...</b>` from `ts_headline`. A small `safeSnippet` helper escapes all HTML then restores only `<b>` and `</b>`.
- Cancellation: `AbortController` aborts in-flight requests when the query changes or the palette closes.

## What was non-obvious

**`d.archived` is `archived_at TIMESTAMPTZ NULL`, not a boolean.** First draft of the search query used `WHERE archived = false`. Caught at integration-test time because fresh test DBs failed with "column does not exist". Fixed to `archived_at IS NULL`. The implementer's debug chain — wrong-column error masked by a `knot-test-support` cached compile artifact pinned to a pre-FTS migration list — is documented inline in the T4 commit.

**Stemmer surprises.** Postgres FTS with `english` config stems aggressively: "Findable" → "findabl", "find" → "find". Searching for "find" does NOT match "Findable". The Plan 7 command-palette e2e was written when the palette did client-side substring matching, where "find" trivially matched "Findable". Plan 14's switch to FTS broke that spec. Fixed by searching for the full word "findable". A future plan adding prefix matching (`to_tsquery` with `:*`) would restore the original UX.

**DocPage's title state doesn't reset on docId change.** Caught by the search e2e: creating multiple docs in sequence via the UI showed stale "previous doc's title" in the input on each iteration. Worked around in the e2e by hitting `/api/docs` directly via `fetch` from inside the page. **This is a real SPA bug** (DocPage uses `useState("")` + an effect that hydrates on first render; remounting on docId would fix it). Not fixed in this plan — separate small follow-up.

**Bundle stayed flat.** The palette change adds ~20 lines of code, no new deps. Bundle: 111 KB gz, unchanged.

**`safeSnippet` HTML sanitization.** Escape-everything-then-restore-allowed-tags is a known-safe pattern for trusted-but-paranoid output. ts_headline only emits `<b>` and `</b>` (configured via `StartSel/StopSel`); the sanitizer permits exactly those and nothing else. Even if a future config change adds more tags, the sanitizer fails closed (escapes them).

## What's still deferred

- **Prefix / wildcard matching.** Would let "find" match "Findable". Needs `to_tsquery` with `:*` suffix plus careful query escaping (plainto_tsquery handles user input safely; to_tsquery requires sanitization). Defer.
- **Vector / semantic search.** pgvector + embeddings. Plan separately if relevance becomes a problem at scale.
- **Faceted search** (filter by author, date, tag). No tag model yet.
- **Multi-language / per-workspace config.** Hard-coded `english` for v0.1.
- **Dedicated search page.** Palette covers the v0.1 use case.
- **Snippet highlighting beyond `<b>`.** Default ts_headline output is fine.
- **DocPage docId stale state fix.** Tracked as carryforward; needs DocPage to reset title state on docId change (or use a key on the component so React remounts it).

## Carryforward for the next plan

In recommended priority order:

1. **Plan 14.5 — DocPage stale-state fix.** ~30 lines. Use `key={id}` on the lazy editor + reset DocPage's title state on docId change. Removes the e2e workaround.
2. **Plan 15 — Mobile / responsive pass.** Sidebar collapse, palette full-screen on narrow viewports, editor toolbar wrapping. Substantial UX work.
3. **Plan 12.5 — Chaos coverage.** Toxiproxy-based WS reconnect e2e (the Plan 12 deferred work).
4. **Plan 16 — Prefix search.** Switch palette to `to_tsquery` with `:*` and sanitization, so partial-word queries match.

## Files of interest

| Path | Role |
|---|---|
| `migrations/20260603104826_fts.sql` | `body_tsv` + `title_tsv` + GIN indexes |
| `crates/knot-storage/src/search.rs` | `SearchStore` trait + `PgSearchStore` |
| `crates/knot-server/src/routes/api/search.rs` | GET /api/search handler with ACL filter |
| `crates/knot-server/tests/search_integration.rs` | 6 cases including snippet + limit + short-query short-circuit |
| `web/src/lib/search.api.ts` | client wrapper |
| `web/src/components/CommandPalette.tsx` | debounced server query + abort + snippet rendering |
| `e2e/flows/search.spec.ts` | new — palette → fill → server search → enter → navigate |
| `e2e/flows/command-palette.spec.ts` | rewritten — adapted to FTS stemming semantics |
