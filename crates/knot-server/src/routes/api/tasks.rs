//! GET /api/workspace/tasks — current user's open tasks across the workspace.
//!
//! Indexed eagerly by the task extractor that runs on each markdown export.
//! Returns rich rows (incl. doc title) so the /tasks page can render a flat
//! list without a separate doc lookup.

use axum::{
    Router,
    extract::{Query, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::auth::AuthContext;
use crate::http_error::json_err;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/workspace/tasks", get(list_mine))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(default)]
    include_completed: bool,
}

#[derive(Debug, Serialize)]
struct TaskRow {
    id: String,
    doc_id: String,
    doc_title: String,
    item_index: i32,
    text: String,
    checked: bool,
    completed_at: Option<String>,
    updated_at: String,
}

async fn list_mine(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
    req: Request,
) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(tasks) = state.tasks.clone() else {
        return internal();
    };
    let Some(docs) = state.docs.clone() else {
        return internal();
    };

    let rows = match tasks
        .list_for_assignee(ctx.workspace_id, ctx.user_id, q.include_completed)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error=?e, "tasks list_for_assignee");
            return internal();
        }
    };

    // Hydrate doc titles in one shot. Tasks are usually scoped to a small
    // set of docs, so a per-row lookup is fine for v1; if it bites we can
    // switch to a JOIN inside the storage layer.
    let mut out: Vec<TaskRow> = Vec::with_capacity(rows.len());
    for t in rows {
        let title = match docs.get(t.doc_id).await {
            Ok(Some(d)) => d.title,
            _ => "(deleted)".to_string(),
        };
        out.push(TaskRow {
            id: t.id,
            doc_id: t.doc_id.to_string(),
            doc_title: title,
            item_index: t.item_index,
            text: t.text,
            checked: t.checked,
            completed_at: t.completed_at.map(|d| d.to_rfc3339()),
            updated_at: t.updated_at.to_rfc3339(),
        });
    }

    axum::Json(out).into_response()
}

fn internal() -> Response {
    json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "")
}
