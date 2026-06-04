//! GET  /api/docs/:id/markdown    → text/markdown export
//! POST /api/docs/:id/markdown    → cold-import markdown as a y-update
//!
//! The room actor is the exclusive owner of the live `DocHandle`, so we only
//! ask it for an encoded state snapshot (`Event::ExportState`) and perform
//! the (potentially expensive) markdown serialization here in the handler
//! against a transient doc. This keeps the actor responsive for editor
//! traffic and avoids polluting the `Engine` trait with a markdown concern.
//!
//! For import we parse the markdown to a y-update in the handler (pure
//! transform) and hand the bytes to the room via `Event::ApplyUpdate`, which
//! applies + persists + fans out to local connections.

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use knot_crdt::{Engine, YrsEngine};
use uuid::Uuid;

use crate::AppState;
use crate::auth::{AuthContext, EffectiveDocRole};
use crate::http_error::json_err;

pub(super) async fn export_inline(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    req: Request,
) -> Response {
    if req.extensions().get::<AuthContext>().is_none() {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    }
    if req.extensions().get::<EffectiveDocRole>().is_none() {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    }
    let Some(rooms) = state.rooms_v2.clone() else {
        return internal();
    };
    let Some(cache) = state.markdown_cache.clone() else {
        return internal();
    };

    let room = rooms.acquire(doc_id).await;
    let (tx, rx) = tokio::sync::oneshot::channel();
    if room
        .tx
        .send(knot_crdt::Event::ExportState(tx))
        .await
        .is_err()
    {
        return internal();
    }
    let (state_bytes, seq) = match rx.await {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            tracing::error!(error=?e, "md export state");
            return internal();
        }
        Err(_) => return internal(),
    };

    // Pure transform over an immutable state snapshot: load into a transient
    // doc and serialize. This intentionally runs off the room actor.
    let engine = YrsEngine;
    let transient = engine.new_doc();
    if let Err(e) = engine.apply_update(&transient, &state_bytes) {
        tracing::error!(error=?e, "md export apply");
        return internal();
    }
    let text = match knot_markdown::to_markdown::serialise(&engine, &transient) {
        Ok(md) => md,
        Err(e) => {
            tracing::error!(error=?e, "md export serialise");
            return internal();
        }
    };

    // Best-effort write-through cache; failure here must not fail the request.
    if let Err(e) = cache.put(doc_id, seq, &text).await {
        tracing::warn!(error=?e, "md cache put failed");
    }

    // Best-effort task re-index so the /tasks page reflects current state.
    if let (Some(tasks), Some(docs)) = (state.tasks.clone(), state.docs.clone()) {
        let extracted = knot_markdown::tasks::extract_tasks(&text);
        let inputs: Vec<knot_storage::DocTaskInput> = extracted
            .into_iter()
            .map(|t| knot_storage::DocTaskInput {
                item_index: t.item_index,
                text: t.text,
                assignee_user_id: t.assignee_user_id,
                checked: t.checked,
            })
            .collect();
        match docs.get(doc_id).await {
            Ok(Some(doc)) => {
                if let Err(e) = tasks.upsert_for_doc(doc.workspace_id, doc_id, &inputs).await {
                    tracing::warn!(error=?e, "task reindex failed");
                }
            }
            _ => {
                tracing::warn!(%doc_id, "task reindex: doc not found");
            }
        }
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/markdown; charset=utf-8")
        .body(Body::from(text))
        .unwrap()
}

pub(super) async fn import_inline(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
    req: Request,
) -> Response {
    let Some(ctx) = req.extensions().get::<AuthContext>().cloned() else {
        return json_err(StatusCode::UNAUTHORIZED, "auth.session_required", "");
    };
    let Some(role) = req.extensions().get::<EffectiveDocRole>().copied() else {
        return json_err(StatusCode::FORBIDDEN, "acl.no_grant", "");
    };
    if role.0 == knot_storage::WorkspaceRole::Viewer {
        return json_err(StatusCode::FORBIDDEN, "acl.editor_required", "");
    }
    let Some(rooms) = state.rooms_v2.clone() else {
        return internal();
    };

    let body = match axum::body::to_bytes(req.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return json_err(StatusCode::BAD_REQUEST, "bad_request", ""),
    };
    let text = match std::str::from_utf8(&body) {
        Ok(s) => s.to_string(),
        Err(_) => return json_err(StatusCode::UNPROCESSABLE_ENTITY, "markdown.not_utf8", ""),
    };

    // Parse markdown to a y-update via knot_markdown. The parse function
    // builds a fresh transient doc and hands us the initial state update
    // bytes; we drop the doc and pass the bytes to the room.
    let update_bytes = match knot_markdown::from_markdown::parse(&text) {
        Ok((_doc, bytes)) => bytes,
        Err(e) => {
            tracing::warn!(error=?e, "md import parse");
            return json_err(StatusCode::UNPROCESSABLE_ENTITY, "markdown.parse", "");
        }
    };

    let room = rooms.acquire(doc_id).await;
    let (tx, rx) = tokio::sync::oneshot::channel();
    if room
        .tx
        .send(knot_crdt::Event::ApplyUpdate {
            update_bytes,
            by_user: Some(ctx.user_id),
            reply: tx,
        })
        .await
        .is_err()
    {
        return internal();
    }
    match rx.await {
        Ok(Ok(_seq)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(e)) => {
            tracing::warn!(error=?e, "md import apply");
            json_err(StatusCode::UNPROCESSABLE_ENTITY, "markdown.apply", "")
        }
        Err(_) => internal(),
    }
}

fn internal() -> Response {
    json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "")
}
