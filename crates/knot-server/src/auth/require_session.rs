//! Rejects requests that have no `AuthContext` with 401 + JSON envelope.
//!
//! Mounted ABOVE routes that demand authentication. The SessionLoader
//! middleware below it populates `AuthContext` from a valid `sid` cookie;
//! if it didn't, this layer turns the request into a 401.

use axum::{
    Json,
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

use super::context::AuthContext;

pub async fn require_session_mw(req: Request<Body>, next: Next) -> Response {
    if req.extensions().get::<AuthContext>().is_some() {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": {
                    "code": "auth.session_required",
                    "message": "session required",
                    "details": {}
                }
            })),
        )
            .into_response()
    }
}
