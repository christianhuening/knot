//! Smoke: server boots, accepts a WS dial.

use std::time::Duration;
use tokio::net::TcpListener;

// Replaced by T20 e2e: the in-memory WS broker is gone. The new
// `collab_upgrade` requires an authenticated session + a Postgres-backed
// Rooms registry. A naked dial against `router()` now correctly fails.
#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn dial_succeeds() {
    use futures_util::SinkExt;
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = knot_server::router();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let url = format!("ws://{addr}/collab/test-doc");
    let (mut ws, _resp) = connect_async(url).await.expect("dial");
    // Server pushes sync-step-2 on connect; read it so we don't deadlock on close.
    use futures_util::StreamExt;
    let _first = ws.next().await;
    ws.send(Message::Close(None)).await.unwrap();
}
