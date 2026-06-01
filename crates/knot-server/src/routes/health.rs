//! Health & meta endpoints.
//!
//! - `/api/healthz` — liveness; always 200 if the process is running.
//! - `/api/readyz` — readiness; 200 only if DB is reachable (or 200
//!   with "ok (in-memory)" body when no pool is configured).
//! - `/api/version` — build metadata.

use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use serde::Serialize;

use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/healthz", get(healthz))
        .route("/api/readyz", get(readyz))
        .route("/api/version", get(version))
}

async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let Some(pool) = state.pool.as_ref() else {
        return (StatusCode::OK, "ok (in-memory)").into_response();
    };
    match sqlx::query("SELECT 1").execute(pool).await {
        Ok(_) => (StatusCode::OK, "ok").into_response(),
        Err(e) => {
            tracing::warn!(error=?e, "readyz: db check failed");
            (StatusCode::SERVICE_UNAVAILABLE, "db unavailable").into_response()
        }
    }
}

#[derive(Serialize)]
struct VersionInfo {
    version: &'static str,
    commit: &'static str,
}

async fn version() -> impl IntoResponse {
    Json(VersionInfo {
        version: env!("KNOT_BUILD_VERSION"),
        commit: env!("KNOT_BUILD_COMMIT"),
    })
}
