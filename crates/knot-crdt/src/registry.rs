//! Rooms registry. T15 fills this in.

use crate::room::RoomHandle;
use dashmap::DashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Default)]
pub struct Rooms {
    map: Arc<DashMap<Uuid, RoomHandle>>,
}

impl Rooms {
    pub fn new() -> Self {
        Self::default()
    }
}

#[allow(dead_code)]
fn _suppress(r: &Rooms) {
    let _ = &r.map;
}
