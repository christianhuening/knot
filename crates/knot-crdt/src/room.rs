//! Per-doc actor. One tokio task. Exclusive owner of `DocHandle` and the
//! local connection map. All I/O flows through mpsc channels.
//!
//! This file is iteratively extended by Tasks 7-15:
//!   T7   minimal select loop + InMsg → engine.apply_update + local fan-out
//!   T8   writer task: batch persist
//!   T9   hydration: load latest snapshot + replay updates
//!   T10  snapshot scheduler
//!   T12  backpressure: bounded channels, slow-consumer close
//!   T13  awareness + bus presence + disconnect clearing
//!   T14  catch-up tick

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::bus::{Bus, Subscription};
use crate::engine::{DocHandle, Engine, EngineError};

pub type ConnId = Uuid;

/// Bytes delivered from a local connection's WS read task.
pub struct InMsg {
    pub from: ConnId,
    pub bytes: Vec<u8>,
}

/// Handle the room hands to a local connection. The WS read task wraps it
/// to send framed messages back to the client.
pub struct ConnHandle {
    pub tx: mpsc::Sender<Vec<u8>>,
}

/// All inputs the room actor multiplexes.
pub enum Event {
    Inbound(InMsg),
    Join {
        conn_id: ConnId,
        handle: ConnHandle,
        reply: oneshot::Sender<Result<Vec<u8>, EngineError>>,
    },
    Leave(ConnId),
    BusUpdate(i64),
    BusPresence(Vec<u8>),
    Shutdown,
}

pub struct Room {
    pub doc_id: Uuid,
    engine: Arc<dyn Engine>,
    doc: DocHandle,
    conns: HashMap<ConnId, ConnHandle>,
    last_applied_seq: i64,
    bus: Arc<dyn Bus>,
    shutdown: CancellationToken,
    rx: mpsc::Receiver<Event>,
    bus_updates_rx: mpsc::Receiver<i64>,
    bus_presence_rx: mpsc::Receiver<Vec<u8>>,
}

pub struct RoomHandle {
    pub tx: mpsc::Sender<Event>,
    pub shutdown: CancellationToken,
}

impl Room {
    /// Spawn a freshly-booted room with an empty doc. T9 will replace this
    /// with hydration from snapshots+updates.
    pub fn spawn(
        doc_id: Uuid,
        engine: Arc<dyn Engine>,
        bus: Arc<dyn Bus>,
        subscription: Subscription,
    ) -> RoomHandle {
        let (tx, rx) = mpsc::channel::<Event>(256);
        let shutdown = CancellationToken::new();
        let doc = engine.new_doc();
        let room = Self {
            doc_id,
            engine,
            doc,
            conns: HashMap::new(),
            last_applied_seq: 0,
            bus,
            shutdown: shutdown.clone(),
            rx,
            bus_updates_rx: subscription.updates,
            bus_presence_rx: subscription.presence,
        };
        tokio::spawn(room.run());
        RoomHandle { tx, shutdown }
    }

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
                    Some(Event::Leave(c)) => { self.conns.remove(&c); }
                    Some(Event::BusUpdate(_)) | Some(Event::BusPresence(_)) => {}
                    Some(Event::Shutdown) | None => break,
                },
                Some(_seq) = self.bus_updates_rx.recv() => {
                    // T14 wires the SELECT-since-watermark replay path.
                }
                Some(_payload) = self.bus_presence_rx.recv() => {
                    // T13 wires presence fan-out.
                }
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

    async fn on_inbound(&mut self, m: InMsg) {
        if let Err(e) = self.engine.apply_update(&self.doc, &m.bytes) {
            tracing::debug!(error=?e, "apply_update failed (T7 stub)");
            return;
        }
        for (cid, conn) in &self.conns {
            if *cid == m.from {
                continue;
            }
            let _ = conn.tx.try_send(m.bytes.clone());
        }
    }
}

// Silence unused warnings on Room fields that T8-T15 will use.
#[allow(dead_code)]
fn _suppress_unused(r: &Room) {
    let _ = &r.bus;
    let _ = &r.last_applied_seq;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemBus, YrsEngine};

    #[tokio::test]
    async fn room_spawns_and_shuts_down_clean() {
        let bus = Arc::new(MemBus::new());
        let doc_id = Uuid::new_v4();
        let sub = bus.subscribe(doc_id).await.unwrap();
        let h = Room::spawn(doc_id, Arc::new(YrsEngine), bus, sub);
        h.shutdown.cancel();
        drop(h);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
