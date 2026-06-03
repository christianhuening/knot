//! Per-board actor. Mirrors `Room` for Excalidraw-style sub-documents but
//! without markdown-cache concerns, snapshot scheduler, or bus integration
//! (single-node for v0.1 — multi-node bus can be added later by wiring the
//! existing `Bus` trait into this loop).
//!
//! v0.1 persistence: the actor calls `BoardStore::append_update` inline
//! when a y-update is applied. Hydration replays the latest snapshot then
//! the update tail. No automatic snapshotting yet — boards are typically
//! small enough that replay is cheap.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::engine::{DocHandle, Engine, EngineError};

/// Wrap raw yrs bytes in a y-sync SYNC_UPDATE frame for the WS broadcast.
///
///   [MSG_SYNC=0] [SYNC_UPDATE=2] [varuint len] [yrs bytes]
fn wrap_sync_update(payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 4);
    out.push(0u8); // MSG_SYNC
    out.push(2u8); // SYNC_UPDATE
    let mut v = payload.len() as u64;
    loop {
        if v < 0x80 {
            out.push(v as u8);
            break;
        }
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.extend_from_slice(payload);
    out
}

pub type ConnId = Uuid;

pub struct InMsg {
    pub from: ConnId,
    pub bytes: Vec<u8>,
}

pub struct ConnHandle {
    pub tx: mpsc::Sender<Vec<u8>>,
}

pub enum Event {
    Inbound(InMsg),
    Join {
        conn_id: ConnId,
        handle: ConnHandle,
        reply: oneshot::Sender<Result<Vec<u8>, EngineError>>,
    },
    Leave(ConnId),
    AwarenessIn {
        from: ConnId,
        payload: Vec<u8>,
    },
    Shutdown,
}

pub struct BoardRoom {
    board_id: Uuid,
    engine: Arc<dyn Engine>,
    doc: DocHandle,
    conns: HashMap<ConnId, ConnHandle>,
    shutdown: CancellationToken,
    rx: mpsc::Receiver<Event>,
    store: Arc<dyn knot_storage::BoardStore>,
}

pub struct BoardRoomHandle {
    pub tx: mpsc::Sender<Event>,
    pub shutdown: CancellationToken,
}

impl BoardRoom {
    pub async fn spawn(
        board_id: Uuid,
        engine: Arc<dyn Engine>,
        store: Arc<dyn knot_storage::BoardStore>,
    ) -> Result<BoardRoomHandle, EngineError> {
        let doc = engine.new_doc();

        // Hydrate from latest snapshot then replay updates after it.
        if let Ok(Some((_seq, state))) = store.latest_snapshot(board_id).await {
            engine.apply_update(&doc, &state)?;
        }
        if let Ok(updates) = store.load_updates(board_id).await {
            for u in updates {
                let _ = engine.apply_update(&doc, &u);
            }
        }

        let (tx, rx) = mpsc::channel::<Event>(256);
        let shutdown = CancellationToken::new();
        let room = Self {
            board_id,
            engine,
            doc,
            conns: HashMap::new(),
            shutdown: shutdown.clone(),
            rx,
            store,
        };
        tokio::spawn(room.run());
        Ok(BoardRoomHandle { tx, shutdown })
    }

    #[tracing::instrument(skip_all, fields(board_id = %self.board_id))]
    async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                _ = self.shutdown.cancelled() => break,
                msg = self.rx.recv() => match msg {
                    Some(Event::Inbound(m)) => self.on_inbound(m).await,
                    Some(Event::Join { conn_id, handle, reply }) => {
                        self.on_join(conn_id, handle, reply).await;
                    }
                    Some(Event::Leave(c)) => {
                        self.conns.remove(&c);
                    }
                    Some(Event::AwarenessIn { from, payload }) => {
                        if payload.len() > 64 * 1024 { continue; }
                        let mut to_close: Vec<ConnId> = Vec::new();
                        for (cid, conn) in &self.conns {
                            if *cid == from { continue; }
                            if conn.tx.try_send(payload.clone()).is_err() {
                                to_close.push(*cid);
                            }
                        }
                        for cid in to_close { self.conns.remove(&cid); }
                    }
                    Some(Event::Shutdown) | None => break,
                },
            }
        }
    }

    async fn on_join(
        &mut self,
        conn_id: ConnId,
        handle: ConnHandle,
        reply: oneshot::Sender<Result<Vec<u8>, EngineError>>,
    ) {
        self.conns.insert(conn_id, handle);
        let r = self.engine.encode_state_as_update(&self.doc, None);
        let _ = reply.send(r);
    }

    #[tracing::instrument(skip(self, m), fields(board_id = %self.board_id, bytes = m.bytes.len()))]
    async fn on_inbound(&mut self, m: InMsg) {
        if let Err(e) = self.engine.apply_update(&self.doc, &m.bytes) {
            tracing::debug!(error=?e, "apply_update failed");
            return;
        }
        // Persist inline — boards are typically small, so the write doesn't
        // need a separate writer task for v0.1.
        if let Err(e) = self.store.append_update(self.board_id, &m.bytes).await {
            tracing::warn!(error=?e, "board append_update failed");
        }
        let framed = wrap_sync_update(&m.bytes);
        let mut to_close: Vec<ConnId> = Vec::new();
        for (cid, conn) in &self.conns {
            if *cid == m.from {
                continue;
            }
            if conn.tx.try_send(framed.clone()).is_err() {
                to_close.push(*cid);
            }
        }
        for cid in to_close {
            self.conns.remove(&cid);
        }
    }
}
