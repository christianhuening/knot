//! In-memory room registry for the spike.
//!
//! Concurrency model: per-room `tokio::sync::Mutex` guarding the Y.Doc
//! and the connection map. Simpler than the actor model, sufficient for
//! the in-memory spike. Plan 5 replaces this with the actor pattern from
//! the Foundation spec §8.3.

use std::{collections::HashMap, sync::Arc};

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use knot_crdt::{DocHandle, Engine, YrsEngine};
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;

use crate::protocol::{YSyncMessage, decode, encode_sync_step2, encode_sync_update};

pub struct Rooms {
    engine: YrsEngine,
    map: Mutex<HashMap<String, Arc<Room>>>,
}

struct Room {
    state: Mutex<RoomState>,
}

struct RoomState {
    doc: DocHandle,
    conns: HashMap<Uuid, mpsc::Sender<Message>>,
}

impl Rooms {
    pub fn new(engine: YrsEngine) -> Self {
        Self {
            engine,
            map: Mutex::new(HashMap::new()),
        }
    }

    async fn get_or_create(&self, doc_id: &str) -> Arc<Room> {
        let mut guard = self.map.lock().await;
        if let Some(r) = guard.get(doc_id) {
            return r.clone();
        }
        let r = Arc::new(Room {
            state: Mutex::new(RoomState {
                doc: self.engine.new_doc(),
                conns: HashMap::new(),
            }),
        });
        guard.insert(doc_id.into(), r.clone());
        r
    }

    pub async fn serve(self: &Arc<Self>, doc_id: String, socket: WebSocket) {
        let room = self.get_or_create(&doc_id).await;
        let conn_id = Uuid::new_v4();
        let (out_tx, mut out_rx) = mpsc::channel::<Message>(256);

        // Register, then send full state as sync-step-2.
        {
            let mut st = room.state.lock().await;
            st.conns.insert(conn_id, out_tx.clone());
            let full = match self.engine.encode_state_as_update(&st.doc, None) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(error=?e, "encode full state failed");
                    return;
                }
            };
            let frame = encode_sync_step2(&full);
            let _ = out_tx.send(Message::Binary(frame)).await;
        }

        let (mut sink, mut stream) = socket.split();
        let writer = tokio::spawn(async move {
            while let Some(msg) = out_rx.recv().await {
                if sink.send(msg).await.is_err() {
                    break;
                }
            }
        });

        while let Some(Ok(msg)) = stream.next().await {
            match msg {
                Message::Binary(bytes) => {
                    if let Err(e) = handle_frame(&self.engine, &room, conn_id, &bytes).await {
                        tracing::warn!(error=?e, "frame handling failed");
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }

        // Cleanup
        {
            let mut st = room.state.lock().await;
            st.conns.remove(&conn_id);
        }
        writer.abort();
    }
}

async fn handle_frame(
    engine: &YrsEngine,
    room: &Room,
    from: Uuid,
    raw: &[u8],
) -> Result<(), anyhow::Error> {
    let msg = decode(raw).map_err(|e| anyhow::anyhow!("decode: {e}"))?;
    match msg {
        YSyncMessage::SyncStep1(peer_sv) => {
            let st = room.state.lock().await;
            let reply = engine
                .encode_state_as_update(&st.doc, Some(&peer_sv))
                .map_err(|e| anyhow::anyhow!("encode missing: {e:?}"))?;
            let frame = encode_sync_step2(&reply);
            if let Some(tx) = st.conns.get(&from) {
                let _ = tx.send(Message::Binary(frame)).await;
            }
        }
        YSyncMessage::SyncStep2(payload) | YSyncMessage::Update(payload) => {
            let st = room.state.lock().await;
            engine
                .apply_update(&st.doc, &payload)
                .map_err(|e| anyhow::anyhow!("apply: {e:?}"))?;
            let out = encode_sync_update(&payload);
            let others: Vec<_> = st
                .conns
                .iter()
                .filter(|(k, _)| **k != from)
                .map(|(_, v)| v.clone())
                .collect();
            drop(st);
            for tx in others {
                let _ = tx.send(Message::Binary(out.clone())).await;
            }
        }
        YSyncMessage::Awareness => {
            // Re-broadcast verbatim (awareness payload is opaque to us).
            let st = room.state.lock().await;
            let others: Vec<_> = st
                .conns
                .iter()
                .filter(|(k, _)| **k != from)
                .map(|(_, v)| v.clone())
                .collect();
            drop(st);
            for tx in others {
                let _ = tx.send(Message::Binary(raw.to_vec())).await;
            }
        }
    }
    Ok(())
}
