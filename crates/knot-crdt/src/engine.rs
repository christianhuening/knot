//! CRDT engine abstraction. The `Engine` trait is what every other crate
//! holds; `YrsEngine` is the v0.1 implementation backed by `yrs`.

use thiserror::Error;
use yrs::{
    Doc, ReadTxn, StateVector, Transact, Update,
    updates::{decoder::Decode, encoder::Encode},
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("yrs apply: {0}")]
    Apply(String),
    #[error("yrs encode: {0}")]
    Encode(String),
}

pub struct DocHandle(pub(crate) Doc);

pub trait Engine: Send + Sync + 'static {
    fn new_doc(&self) -> DocHandle;
    fn apply_update(&self, d: &DocHandle, update: &[u8]) -> Result<(), EngineError>;
    fn encode_state_as_update(
        &self,
        d: &DocHandle,
        peer_sv: Option<&[u8]>,
    ) -> Result<Vec<u8>, EngineError>;
    fn encode_state_vector(&self, d: &DocHandle) -> Result<Vec<u8>, EngineError>;
}

#[derive(Default, Clone)]
pub struct YrsEngine;

impl Engine for YrsEngine {
    fn new_doc(&self) -> DocHandle {
        DocHandle(Doc::new())
    }

    fn apply_update(&self, d: &DocHandle, update: &[u8]) -> Result<(), EngineError> {
        let u = Update::decode_v1(update).map_err(|e| EngineError::Apply(e.to_string()))?;
        let mut txn = d.0.transact_mut();
        txn.apply_update(u)
            .map_err(|e| EngineError::Apply(e.to_string()))?;
        Ok(())
    }

    fn encode_state_as_update(
        &self,
        d: &DocHandle,
        peer_sv: Option<&[u8]>,
    ) -> Result<Vec<u8>, EngineError> {
        let sv = match peer_sv {
            Some(bytes) => {
                StateVector::decode_v1(bytes).map_err(|e| EngineError::Encode(e.to_string()))?
            }
            None => StateVector::default(),
        };
        let txn = d.0.transact();
        Ok(txn.encode_state_as_update_v1(&sv))
    }

    fn encode_state_vector(&self, d: &DocHandle) -> Result<Vec<u8>, EngineError> {
        let txn = d.0.transact();
        Ok(txn.state_vector().encode_v1())
    }
}

#[derive(Debug, Clone)]
pub struct TextMark {
    pub kind: String,
    pub attrs: Vec<TextMarkAttr>,
}

#[derive(Debug, Clone)]
pub struct TextMarkAttr {
    pub name: String,
    pub value: String,
}
