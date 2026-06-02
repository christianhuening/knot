//! Shared JSON error envelope helper per spec §6.3:
//! `{"error": {"code": "<stable>", "message": "<human>", "details": {}}}`

use axum::{Json, http::StatusCode, response::IntoResponse, response::Response};
use serde_json::json;

pub fn json_err(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({
            "error": {
                "code": code,
                "message": message,
                "details": {},
            }
        })),
    )
        .into_response()
}
