//! knot spike server library — exports `router()` for tests.
//!
//! In-memory, no persistence, no auth. v0.1 of the Foundation Spike.

use std::sync::Arc;

use axum::{
    Router,
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use knot_crdt::YrsEngine;

pub mod protocol;
pub mod room;

use room::Rooms;

#[derive(Clone)]
pub struct AppState {
    pub rooms: Arc<Rooms>,
}

pub fn router() -> Router {
    let state = AppState {
        rooms: Arc::new(Rooms::new(YrsEngine)),
    };
    Router::new()
        .route("/collab/:doc_id", get(collab_upgrade))
        .with_state(state)
}

async fn collab_upgrade(
    Path(doc_id): Path<String>,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        state.rooms.serve(doc_id, socket).await;
    })
}
