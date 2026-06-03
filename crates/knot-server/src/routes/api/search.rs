//! GET /api/search?q=<query>&limit=<n>
//!
//! Searches docs in the user's workspace by title + body. ACL-filtered:
//! re-checks `effective_role` on each candidate so revoked grants don't
//! leak through the FTS layer.

use std::collections::HashSet;

use axum::{
    Json, Router,
    extract::{Query, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};

use crate::AppState;
use crate::auth::AuthContext;
use crate::http_error::json_err;

#[derive(serde::Deserialize)]
struct SearchParams {
    q: String,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(serde::Serialize)]
struct SearchResponse {
    results: Vec<knot_storage::SearchHit>,
}

const MAX_LIMIT: i64 = 20;
const MIN_QUERY_LEN: usize = 2;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/search", get(search))
}

async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
    req: Request,
) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let q = params.q.trim().to_string();
    if q.chars().count() < MIN_QUERY_LEN {
        return Json(SearchResponse { results: vec![] }).into_response();
    }
    let limit = params.limit.unwrap_or(MAX_LIMIT).clamp(1, MAX_LIMIT);

    let Some(search) = state.search.clone() else {
        return json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "");
    };
    let Some(acl) = state.acl.clone() else {
        return json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "");
    };

    // Pull a bit more than the final limit so the ACL filter has room.
    let raw = match search.search(ctx.workspace_id, &q, limit * 2).await {
        Ok(r) => r,
        Err(_) => return json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", ""),
    };

    let mut allowed = Vec::with_capacity(raw.len());
    let mut seen = HashSet::new();
    for hit in raw {
        if !seen.insert(hit.doc_id) {
            continue;
        }
        match acl
            .effective_role(ctx.workspace_id, hit.doc_id, ctx.user_id)
            .await
        {
            Ok(Some(_)) => allowed.push(hit),
            _ => continue,
        }
        if allowed.len() >= limit as usize {
            break;
        }
    }
    Json(SearchResponse { results: allowed }).into_response()
}
