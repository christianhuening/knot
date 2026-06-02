//! Rejects requests that have no `AuthContext` with 401 + JSON envelope.

use axum::{body::Body, extract::Request, http::StatusCode, middleware::Next, response::Response};

use super::context::AuthContext;
use crate::http_error::json_err;

pub async fn require_session_mw(req: Request<Body>, next: Next) -> Response {
    if req.extensions().get::<AuthContext>().is_some() {
        next.run(req).await
    } else {
        json_err(
            StatusCode::UNAUTHORIZED,
            "auth.session_required",
            "session required",
        )
    }
}
