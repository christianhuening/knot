//! Integration tests for health endpoints.

use std::time::Duration;
use tokio::net::TcpListener;

async fn spawn_app() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = knot_server::router();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(30)).await;
    format!("http://{addr}")
}

#[tokio::test(flavor = "multi_thread")]
async fn healthz_returns_ok() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/api/healthz"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), "ok");
}

#[tokio::test(flavor = "multi_thread")]
async fn readyz_returns_ok_without_pool() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/api/readyz"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();
    assert!(body.contains("ok"));
}

#[tokio::test(flavor = "multi_thread")]
async fn version_returns_json() {
    let base = spawn_app().await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/api/version"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("version").is_some());
    assert!(body.get("commit").is_some());
}
