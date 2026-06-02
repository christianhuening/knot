//! Documents API:
//! - GET    /api/docs            flat list (alive only)
//! - POST   /api/docs            body: {title?, parent_id?, after_id?}
//! - GET    /api/docs/:id        metadata + effective_role
//!
//! PATCH/DELETE/move/restore land in T13/T14 (handlers stubbed below for
//! router shape; replaced in later tasks).

use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use knot_storage::{Document, WorkspaceRole, sort_key_between};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::auth::{AuthContext, EffectiveDocRole, require_doc_role_mw};
use crate::http_error::json_err;

#[derive(Serialize)]
struct DocResponse {
    id: String,
    workspace_id: String,
    parent_id: Option<String>,
    title: String,
    sort_key: String,
    icon: Option<String>,
    created_by: String,
    archived: bool,
}

fn to_response(d: &Document) -> DocResponse {
    DocResponse {
        id: d.id.to_string(),
        workspace_id: d.workspace_id.to_string(),
        parent_id: d.parent_id.map(|u| u.to_string()),
        title: d.title.clone(),
        sort_key: d.sort_key.clone(),
        icon: d.icon.clone(),
        created_by: d.created_by.to_string(),
        archived: d.archived_at.is_some(),
    }
}

pub fn router(state: AppState) -> Router<AppState> {
    let doc_id_routes: Router<AppState> = Router::new()
        .route("/api/docs/:id", get(get_one).patch(rename).delete(archive))
        .route("/api/docs/:id/move", post(move_doc))
        .route("/api/docs/:id/restore", post(restore))
        .layer(middleware::from_fn_with_state(state, require_doc_role_mw));
    let list_routes: Router<AppState> = Router::new().route("/api/docs", get(list).post(create));
    list_routes.merge(doc_id_routes)
}

async fn list(State(state): State<AppState>, req: Request) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(docs) = state.docs.clone() else {
        return internal();
    };
    match docs.list_alive(ctx.workspace_id).await {
        Ok(list) => Json(list.iter().map(to_response).collect::<Vec<_>>()).into_response(),
        Err(e) => {
            tracing::error!(error=?e, "list");
            internal()
        }
    }
}

#[derive(Deserialize)]
struct CreateRequest {
    title: Option<String>,
    parent_id: Option<Uuid>,
    after_id: Option<Uuid>,
}

async fn create(State(state): State<AppState>, req: Request) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    if ctx.role == WorkspaceRole::Viewer {
        return json_err(StatusCode::FORBIDDEN, "acl.editor_required", "");
    }
    let Ok(body) = read_json::<CreateRequest>(req).await else {
        return json_err(StatusCode::BAD_REQUEST, "bad_request", "");
    };
    let Some(docs) = state.docs.clone() else {
        return internal();
    };
    let title = body.title.unwrap_or_else(|| "Untitled".into());

    let siblings = match docs.siblings(ctx.workspace_id, body.parent_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error=?e, "siblings");
            return internal();
        }
    };
    let (a, b) = match body.after_id {
        None => (None, siblings.first().map(|d| d.sort_key.as_str())),
        Some(aid) => {
            let i = siblings.iter().position(|d| d.id == aid);
            match i {
                Some(i) => (
                    Some(siblings[i].sort_key.as_str()),
                    siblings.get(i + 1).map(|d| d.sort_key.as_str()),
                ),
                None => (siblings.last().map(|d| d.sort_key.as_str()), None),
            }
        }
    };
    let sk = sort_key_between(a, b);

    match docs
        .create(ctx.workspace_id, body.parent_id, &title, &sk, ctx.user_id)
        .await
    {
        Ok(d) => (StatusCode::CREATED, Json(to_response(&d))).into_response(),
        Err(e) => {
            tracing::error!(error=?e, "create");
            internal()
        }
    }
}

async fn get_one(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    req: Request,
) -> Response {
    if req.extensions().get::<AuthContext>().is_none() {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    }
    let Some(role) = req.extensions().get::<EffectiveDocRole>().copied() else {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    };
    let Some(docs) = state.docs.clone() else {
        return internal();
    };
    let doc = match docs.get(doc_id).await {
        Ok(Some(d)) => d,
        Ok(None) => return json_err(StatusCode::NOT_FOUND, "doc.not_found", ""),
        Err(e) => {
            tracing::error!(error=?e, "get");
            return internal();
        }
    };
    #[derive(Serialize)]
    struct GetResponse {
        #[serde(flatten)]
        doc: DocResponse,
        effective_role: String,
    }
    Json(GetResponse {
        doc: to_response(&doc),
        effective_role: role.0.as_str().into(),
    })
    .into_response()
}

// PATCH/DELETE/move/restore stubs — T13/T14 will implement.
async fn rename(_p: Path<Uuid>) -> Response {
    json_err(StatusCode::NOT_IMPLEMENTED, "not_implemented", "")
}
async fn move_doc(_p: Path<Uuid>) -> Response {
    json_err(StatusCode::NOT_IMPLEMENTED, "not_implemented", "")
}
async fn archive(_p: Path<Uuid>) -> Response {
    json_err(StatusCode::NOT_IMPLEMENTED, "not_implemented", "")
}
async fn restore(_p: Path<Uuid>) -> Response {
    json_err(StatusCode::NOT_IMPLEMENTED, "not_implemented", "")
}

async fn read_json<T: serde::de::DeserializeOwned>(req: Request) -> Result<T, ()> {
    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
        .await
        .map_err(|_| ())?;
    serde_json::from_slice(&bytes).map_err(|_| ())
}

fn internal() -> Response {
    json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "")
}
