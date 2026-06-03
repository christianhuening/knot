-- fts
-- Created 2026-06-03

-- doc_markdown_cache.body_tsv — full-text index of the cached markdown.
-- The markdown cache lags live editor state until the next snapshot, so
-- search is eventually consistent with edits. Acceptable for v0.1.
ALTER TABLE doc_markdown_cache
  ADD COLUMN body_tsv tsvector
  GENERATED ALWAYS AS (to_tsvector('english', coalesce(markdown_text, ''))) STORED;
CREATE INDEX doc_markdown_cache_body_tsv_idx
  ON doc_markdown_cache USING GIN (body_tsv);

-- documents.title_tsv — title-only index. Smaller and faster than body,
-- and we always have a title even before the body cache is populated.
ALTER TABLE documents
  ADD COLUMN title_tsv tsvector
  GENERATED ALWAYS AS (to_tsvector('english', coalesce(title, ''))) STORED;
CREATE INDEX documents_title_tsv_idx
  ON documents USING GIN (title_tsv);
