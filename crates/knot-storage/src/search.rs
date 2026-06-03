//! Postgres-backed full-text search across docs in a workspace.
//!
//! Indexes:
//! - `documents.title_tsv` (STORED GENERATED, english)
//! - `doc_markdown_cache.body_tsv` (STORED GENERATED, english)
//!
//! Body search is eventually consistent — the cache lags live editor
//! state until the next snapshot. v0.1 accepts this lag.

use async_trait::async_trait;
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SearchStoreError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchHit {
    pub doc_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub title: String,
    pub snippet: String,
    pub rank: f32,
}

#[async_trait]
pub trait SearchStore: Send + Sync {
    async fn search(
        &self,
        workspace_id: Uuid,
        q: &str,
        limit: i64,
    ) -> Result<Vec<SearchHit>, SearchStoreError>;
}

pub struct PgSearchStore {
    pool: PgPool,
}

impl PgSearchStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SearchStore for PgSearchStore {
    async fn search(
        &self,
        workspace_id: Uuid,
        q: &str,
        limit: i64,
    ) -> Result<Vec<SearchHit>, SearchStoreError> {
        let rows: Vec<(Uuid, Option<Uuid>, String, Option<String>, f32)> = sqlx::query_as(
            r#"
            SELECT d.id,
                   d.parent_id,
                   d.title,
                   CASE
                     WHEN c.body_tsv @@ plainto_tsquery('english', $2) THEN
                       ts_headline('english', c.markdown_text,
                                   plainto_tsquery('english', $2),
                                   'MaxFragments=2,MinWords=5,MaxWords=15,StartSel=<b>,StopSel=</b>')
                     ELSE NULL
                   END AS snippet,
                   GREATEST(
                     COALESCE(ts_rank_cd(d.title_tsv, plainto_tsquery('english', $2)), 0.0) * 2.0,
                     COALESCE(ts_rank_cd(c.body_tsv,  plainto_tsquery('english', $2)), 0.0)
                   )::real AS rank
              FROM documents d
              LEFT JOIN doc_markdown_cache c ON c.doc_id = d.id
             WHERE d.workspace_id = $1
               AND d.archived_at IS NULL
               AND (
                     d.title_tsv @@ plainto_tsquery('english', $2)
                  OR c.body_tsv  @@ plainto_tsquery('english', $2)
                   )
             ORDER BY rank DESC
             LIMIT $3
            "#,
        )
        .bind(workspace_id)
        .bind(q)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(doc_id, parent_id, title, snippet, rank)| SearchHit {
                doc_id,
                parent_id,
                title,
                snippet: snippet.unwrap_or_default(),
                rank,
            })
            .collect())
    }
}
