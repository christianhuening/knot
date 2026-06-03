//! Scrape /metrics after generating some HTTP traffic.
//!
//! NB: `/metrics` is served on a SEPARATE port (KNOT_METRICS_ADDR /
//! default :9090) — it is NOT on the main axum router. We install the
//! exporter on a random port, drive the app via oneshot, then scrape
//! over HTTP.
//!
//! The Prometheus recorder is a global singleton. This file is structured
//! so all tests share a single recorder install via std::sync::OnceLock,
//! which keeps the suite robust when cargo runs multiple tests in one
//! process.

use std::sync::OnceLock;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use knot_test_support::fresh_db;
use tower::ServiceExt;

static EXPORTER_PORT: OnceLock<u16> = OnceLock::new();

fn install_exporter() -> u16 {
    *EXPORTER_PORT.get_or_init(|| {
        let port = pick_free_port();
        let addr = format!("127.0.0.1:{port}");
        knot_obs::metrics::init(&addr).expect("install exporter");
        port
    })
}

fn pick_free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let p = l.local_addr().unwrap().port();
    drop(l);
    p
}

#[tokio::test(flavor = "multi_thread")]
async fn metrics_endpoint_lists_described_names_after_traffic() {
    let port = install_exporter();
    let db = fresh_db().await;
    let mut state = knot_server::AppState::with_pool(db.pool.clone());
    state.session_key = b"test-key-32-bytes-aaaaaaaaaaaaaa".to_vec();
    let app = knot_server::router_with_state(state);

    // Generate traffic on a couple of routes.
    for _ in 0..3 {
        let r = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
    }

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let body = reqwest::get(format!("http://127.0.0.1:{port}/metrics"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    // Described families appear in the output even with zero samples;
    // ones we actually drove also include sample lines.
    assert!(
        body.contains("knot_http_requests_total"),
        "missing knot_http_requests_total\n{body}"
    );
    assert!(
        body.contains("knot_http_request_duration_seconds"),
        "missing knot_http_request_duration_seconds\n{body}"
    );
    // knot_room_active is a gauge that only appears after a sample is recorded;
    // skip asserting on it here since no room traffic is generated in this test.
}
