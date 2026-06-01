//! Tower-level oneshot tests for the auth middleware layer.

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use knot_server::auth::{AuthContext, require_session_mw};
use knot_storage::WorkspaceRole;
use tower::ServiceExt;
use uuid::Uuid;

async fn ok() -> &'static str {
    "ok"
}

#[tokio::test]
async fn require_session_rejects_when_no_auth_context() {
    let app = Router::new()
        .route("/x", get(ok))
        .layer(axum::middleware::from_fn(require_session_mw));
    let resp = app
        .oneshot(Request::builder().uri("/x").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn require_session_allows_when_auth_context_present() {
    // Inject AuthContext into request extensions via a from_fn layer that
    // wraps the request before require_session_mw runs.
    let app = Router::new()
        .route("/x", get(ok))
        // Layer order: from bottom up. The injecting layer must run BEFORE
        // require_session_mw so the extension is visible at the check.
        .layer(axum::middleware::from_fn(require_session_mw))
        .layer(axum::middleware::from_fn(
            |mut req: Request<Body>, next: axum::middleware::Next| async move {
                req.extensions_mut().insert(AuthContext {
                    user_id: Uuid::new_v4(),
                    workspace_id: Uuid::new_v4(),
                    role: WorkspaceRole::Owner,
                });
                next.run(req).await
            },
        ));

    let resp = app
        .oneshot(Request::builder().uri("/x").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
