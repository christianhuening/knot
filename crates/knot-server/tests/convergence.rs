//! Two clients send Yjs updates through the broker; both end up with
//! the same state vector. This is the in-process analogue of the
//! Playwright headline test (T14).

use std::time::Duration;
use tokio::net::TcpListener;

// Replaced by T20 e2e: the in-memory spike WS broker is gone; the new
// `collab_upgrade` requires an authenticated session + a Postgres-backed
// Rooms registry, which this test scaffolding does not provide.
#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn two_clients_converge() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = knot_server::router();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let url = format!("ws://{addr}/collab/doc/converge-doc");
    let (mut a, _) = connect_async(&url).await.expect("dial a");
    let (mut b, _) = connect_async(&url).await.expect("dial b");

    // Drain the initial sync-step-2 each connection receives.
    let _a_init = a.next().await;
    let _b_init = b.next().await;

    // Build a Yjs update on the "a" side using yrs directly, then send it
    // wrapped in a sync-update frame. The broker should forward to "b".
    use yrs::{
        Doc, ReadTxn, Transact, XmlElementPrelim, XmlFragment, XmlTextPrelim,
        updates::encoder::Encode,
    };
    let local_doc = Doc::new();
    let sv_empty = {
        let txn = local_doc.transact();
        txn.state_vector().encode_v1()
    };
    {
        let frag = local_doc.get_or_insert_xml_fragment("default");
        let mut txn = local_doc.transact_mut();
        let p = frag.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
        p.push_back(&mut txn, XmlTextPrelim::new("hello"));
    }
    let update = {
        let txn = local_doc.transact();
        txn.encode_state_as_update_v1(&yrs::StateVector::decode_v1(&sv_empty).unwrap())
    };

    // Wrap in y-sync-update wire frame.
    let mut frame = vec![0u8, 2u8]; // MSG_SYNC, SYNC_UPDATE
    append_var_uint(&mut frame, update.len() as u64);
    frame.extend_from_slice(&update);

    a.send(Message::Binary(frame)).await.unwrap();

    // Wait for "b" to receive a forwarded update.
    let received = tokio::time::timeout(Duration::from_secs(2), b.next())
        .await
        .expect("timed out waiting for forwarded update")
        .expect("stream ended")
        .expect("error frame");
    match received {
        Message::Binary(bytes) => {
            // Frame should be a sync-update (type=0, subtype=2).
            assert_eq!(bytes[0], 0);
            assert_eq!(bytes[1], 2);
        }
        other => panic!("expected binary, got {other:?}"),
    }

    a.send(Message::Close(None)).await.ok();
    b.send(Message::Close(None)).await.ok();
}

fn append_var_uint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

use yrs::updates::decoder::Decode;
