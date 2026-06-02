//! GET  /api/docs/:id/markdown    → text/markdown export
//!
//! The room actor is the exclusive owner of the live `DocHandle`, so we only
//! ask it for an encoded state snapshot (`Event::ExportState`) and perform
//! the (potentially expensive) markdown serialization here in the handler
//! against a transient doc. This keeps the actor responsive for editor
//! traffic and avoids polluting the `Engine` trait with a markdown concern.

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    response::Response,
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

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/markdown; charset=utf-8")
        .body(Body::from(text))
        .unwrap()
}

fn internal() -> Response {
    json_err(StatusCode::INTERNAL_SERVER_ERROR, "internal", "")
}
