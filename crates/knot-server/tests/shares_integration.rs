//! Integration tests for share-token creation, revocation, ACL, expiry, and
//! anonymous public access via /p/:token.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use knot_auth::{Hasher, Throttle};
use knot_server::{router_with_state, AppState};
use knot_storage::WorkspaceRole;
use tower::ServiceExt;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared scaffolding
// ---------------------------------------------------------------------------

/// Seed a workspace + alice user with the given role, create a doc.
/// Returns `(state, workspace_id, doc_id, user_id)`.
async fn state_with_seeded(role: WorkspaceRole) -> (AppState, Uuid, Uuid, Uuid) {
    let pool = knot_test_support::fresh_db().await.pool;
    let mut s = AppState::with_pool(pool.clone());
    s.hasher = Arc::new(Hasher::fast_for_tests());
    s.throttle = Arc::new(Throttle::new());
    s.session_key = b"test-key-32-bytes-aaaaaaaaaaaaaa".to_vec();

    let hash = s.hasher.hash("hunter22").unwrap();
    let ws = s
        .workspaces
        .as_ref()
        .unwrap()
        .create("default", "Workspace")
        .await
        .unwrap();
    let user = s
        .users
        .as_ref()
        .unwrap()
        .create_local("alice@example.com", "Alice", &hash)
        .await
        .unwrap();
    s.workspaces
        .as_ref()
        .unwrap()
        .add_member(ws.id, user.id, role)
        .await
        .unwrap();

    let doc = s
        .docs
        .as_ref()
        .unwrap()
        .create(ws.id, None, "Share Me", "m", user.id)
        .await
        .unwrap();

    (s, ws.id, doc.id, user.id)
}

/// Log in as alice and return `(sid_kv, csrf_val)`.
async fn login_alice(app: &axum::Router) -> (String, String) {
    let r = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"email": "alice@example.com", "password": "hunter22"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    let cookies: Vec<String> = r
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();
    let sid_kv = cookies
        .iter()
        .find(|c| c.starts_with("sid="))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let csrf_val = cookies
        .iter()
        .find(|c| c.starts_with("csrf="))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .split('=')
        .nth(1)
        .unwrap()
        .to_string();
    (sid_kv, csrf_val)
}

async fn create_share(
    app: &axum::Router,
    sid: &str,
    csrf: &str,
    doc_id: Uuid,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let r = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/docs/{doc_id}/shares"))
                .header("content-type", "application/json")
                .header("cookie", format!("{sid}; csrf={csrf}"))
                .header("x-csrf-token", csrf)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = r.status();
    let bytes = r.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::json!({}));
    (status, json)
}

async fn revoke_share(
    app: &axum::Router,
    sid: &str,
    csrf: &str,
    doc_id: Uuid,
    share_id: &str,
) -> StatusCode {
    let r = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/docs/{doc_id}/shares/{share_id}"))
                .header("cookie", format!("{sid}; csrf={csrf}"))
                .header("x-csrf-token", csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    r.status()
}

async fn anon_get_token(app: &axum::Router, token: &str) -> (StatusCode, String) {
    let r = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/p/{token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = r.status();
    let bytes = r.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// 1. Owner creates a share token — 201 with expected fields.
#[tokio::test(flavor = "multi_thread")]
async fn owner_creates_token_201() {
    let (state, _ws, doc_id, _u) = state_with_seeded(WorkspaceRole::Owner).await;
    let app = router_with_state(state);
    let (sid, csrf) = login_alice(&app).await;

    let (status, json) = create_share(&app, &sid, &csrf, doc_id, serde_json::json!({})).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(
        json["token"].as_str().is_some(),
        "expected token field: {json}"
    );
    assert!(
        json["url"].as_str().is_some(),
        "expected url field: {json}"
    );
    assert!(json["expires_at"].is_null(), "expected null expires_at");
    assert!(
        json["created_at"].as_str().is_some(),
        "expected created_at field: {json}"
    );
}

/// 2. Editor cannot create a share token — 403 acl.no_grant.
#[tokio::test(flavor = "multi_thread")]
async fn editor_cannot_create_403() {
    let (state, _ws, doc_id, _u) = state_with_seeded(WorkspaceRole::Editor).await;
    let app = router_with_state(state);
    let (sid, csrf) = login_alice(&app).await;

    let (status, json) = create_share(&app, &sid, &csrf, doc_id, serde_json::json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["error"]["code"], "acl.no_grant");
}

/// 3. Viewer cannot create a share token — 403 acl.no_grant.
#[tokio::test(flavor = "multi_thread")]
async fn viewer_cannot_create_403() {
    let (state, _ws, doc_id, _u) = state_with_seeded(WorkspaceRole::Viewer).await;
    let app = router_with_state(state);
    let (sid, csrf) = login_alice(&app).await;

    let (status, json) = create_share(&app, &sid, &csrf, doc_id, serde_json::json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["error"]["code"], "acl.no_grant");
}

/// 4. Anon GET valid token returns 200 HTML containing the doc title.
#[tokio::test(flavor = "multi_thread")]
async fn anon_get_valid_token_200_html() {
    let (state, _ws, doc_id, _u) = state_with_seeded(WorkspaceRole::Owner).await;
    // Seed the markdown cache before building the router.
    state
        .markdown_cache
        .as_ref()
        .unwrap()
        .put(doc_id, 1, "# Share Me\n\nbody text")
        .await
        .unwrap();
    let app = router_with_state(state);
    let (sid, csrf) = login_alice(&app).await;

    let (status, json) = create_share(&app, &sid, &csrf, doc_id, serde_json::json!({})).await;
    assert_eq!(status, StatusCode::CREATED);
    let token = json["token"].as_str().unwrap().to_string();

    let (status, body) = anon_get_token(&app, &token).await;
    assert_eq!(status, StatusCode::OK);
    // Content-type header check via the response — we already consumed it,
    // but the body should be HTML.
    assert!(
        body.contains("text/html") || body.contains("<html"),
        "expected HTML body"
    );
    assert!(body.contains("Share Me"), "expected doc title in body");
}

/// 5. Anon GET unknown token returns 410.
#[tokio::test(flavor = "multi_thread")]
async fn anon_get_unknown_token_410() {
    let (state, _ws, _doc, _u) = state_with_seeded(WorkspaceRole::Owner).await;
    let app = router_with_state(state);

    let (status, _) = anon_get_token(&app, "totally-invalid-token-xyz").await;
    assert_eq!(status, StatusCode::GONE);
}

/// 6. Owner revokes token — subsequent anon GET returns 410.
#[tokio::test(flavor = "multi_thread")]
async fn owner_revokes_then_anon_gets_410() {
    let (state, _ws, doc_id, _u) = state_with_seeded(WorkspaceRole::Owner).await;
    state
        .markdown_cache
        .as_ref()
        .unwrap()
        .put(doc_id, 1, "# Share Me\n\nbody text")
        .await
        .unwrap();
    let app = router_with_state(state);
    let (sid, csrf) = login_alice(&app).await;

    let (status, json) = create_share(&app, &sid, &csrf, doc_id, serde_json::json!({})).await;
    assert_eq!(status, StatusCode::CREATED);
    let token = json["token"].as_str().unwrap().to_string();
    let share_id = json["id"].as_str().unwrap().to_string();

    // Confirm it works before revocation.
    let (status, _) = anon_get_token(&app, &token).await;
    assert_eq!(status, StatusCode::OK);

    // Revoke.
    let del_status = revoke_share(&app, &sid, &csrf, doc_id, &share_id).await;
    assert_eq!(del_status, StatusCode::NO_CONTENT);

    // Now anon GET should be 410.
    let (status, _) = anon_get_token(&app, &token).await;
    assert_eq!(status, StatusCode::GONE);
}

/// 7. Expired token (expires_at 1 hour in the past) returns 410.
#[tokio::test(flavor = "multi_thread")]
async fn expired_token_410() {
    let (state, _ws, doc_id, _u) = state_with_seeded(WorkspaceRole::Owner).await;
    state
        .markdown_cache
        .as_ref()
        .unwrap()
        .put(doc_id, 1, "# Share Me\n\nbody text")
        .await
        .unwrap();
    let app = router_with_state(state);
    let (sid, csrf) = login_alice(&app).await;

    let past = chrono::Utc::now() - chrono::Duration::hours(1);
    let body = serde_json::json!({ "expires_at": past.to_rfc3339() });
    let (status, json) = create_share(&app, &sid, &csrf, doc_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let token = json["token"].as_str().unwrap().to_string();

    let (status, _) = anon_get_token(&app, &token).await;
    assert_eq!(status, StatusCode::GONE);
}

/// 8. Future expiry token returns 200.
#[tokio::test(flavor = "multi_thread")]
async fn future_expiry_token_200() {
    let (state, _ws, doc_id, _u) = state_with_seeded(WorkspaceRole::Owner).await;
    state
        .markdown_cache
        .as_ref()
        .unwrap()
        .put(doc_id, 1, "# Share Me\n\nbody text")
        .await
        .unwrap();
    let app = router_with_state(state);
    let (sid, csrf) = login_alice(&app).await;

    let future = chrono::Utc::now() + chrono::Duration::hours(1);
    let body = serde_json::json!({ "expires_at": future.to_rfc3339() });
    let (status, json) = create_share(&app, &sid, &csrf, doc_id, body).await;
    assert_eq!(status, StatusCode::CREATED);
    let token = json["token"].as_str().unwrap().to_string();

    let (status, _) = anon_get_token(&app, &token).await;
    assert_eq!(status, StatusCode::OK);
}

/// 9. No markdown cache — anon GET returns 503.
#[tokio::test(flavor = "multi_thread")]
async fn no_markdown_cache_503() {
    let (state, _ws, doc_id, _u) = state_with_seeded(WorkspaceRole::Owner).await;
    // Deliberately do NOT seed the markdown cache.
    let app = router_with_state(state);
    let (sid, csrf) = login_alice(&app).await;

    let (status, json) = create_share(&app, &sid, &csrf, doc_id, serde_json::json!({})).await;
    assert_eq!(status, StatusCode::CREATED);
    let token = json["token"].as_str().unwrap().to_string();

    let (status, _) = anon_get_token(&app, &token).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
