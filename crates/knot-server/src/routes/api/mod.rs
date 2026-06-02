//! `/api/*` routes. Auth + CSRF middlewares are layered here in T11.

use axum::Router;

use crate::AppState;

pub mod workspace;

pub fn router() -> Router<AppState> {
    Router::new().merge(workspace::router())
}
